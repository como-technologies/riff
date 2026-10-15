#!/usr/bin/env python3
"""The stand-in of `riff relay` for the spike of O2 (#707, #699).

It starts a command (podman run -i ... claude -p --input-format
stream-json --output-format stream-json --verbose), writes each message
on its stdin as one stream-json line, and reads the events from its
stdout until the result of that turn. It keeps stdin open between the
turns, as the relay does between two wakes. Then it closes stdin and
waits for the end.

It writes each event to --log, and a summary in Markdown to --summary.
It checks that no secret of --secret-env or --secret-file is in the
log. It never prints a secret: it names the secret and removes it from
the log.
"""

import argparse
import collections
import json
import os
import queue
import subprocess
import sys
import threading
import time


def user_line(text):
    """One stream-json input line with the text of a person."""
    message = {"role": "user", "content": [{"type": "text", "text": text}]}
    return json.dumps({"type": "user", "message": message}) + "\n"


def short_input(name, data):
    """The part of a tool input that tells what the call does."""
    if not isinstance(data, dict):
        return ""
    for key in ("command", "file_path", "pattern", "path", "url"):
        if key in data:
            return str(data[key]).replace("\n", " ")[:160]
    return ",".join(sorted(data.keys()))


def secrets(env_names, file_paths):
    """The secrets to look for in the log: name, value."""
    found = []
    for name in env_names:
        value = os.environ.get(name, "")
        if len(value) >= 8:
            found.append((name, value))
    for path in file_paths:
        try:
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
        except (OSError, ValueError):
            continue
        stack = [(os.path.basename(path), data)]
        while stack:
            where, item = stack.pop()
            if isinstance(item, dict):
                for key, value in item.items():
                    stack.append((f"{where}.{key}", value))
            elif isinstance(item, str) and "token" in where.lower() and len(item) >= 8:
                found.append((where, item))
    return found


def reader(stream, out):
    for line in stream:
        out.put(line)
    out.put(None)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--log", required=True, help="the events, one JSON line each")
    parser.add_argument("--stderr", required=True, help="the stderr of the command")
    parser.add_argument("--summary", required=True, help="the summary in Markdown")
    parser.add_argument("--message", action="append", required=True,
                        help="a file with the text of one turn; give it once for each turn")
    parser.add_argument("--turn-timeout", type=int, default=900,
                        help="seconds to wait for the result of one turn")
    parser.add_argument("--secret-env", action="append", default=[],
                        help="the name of a variable whose value must not be in the log")
    parser.add_argument("--secret-file", action="append", default=[],
                        help="a JSON file whose token values must not be in the log")
    parser.add_argument("command", nargs=argparse.REMAINDER,
                        help="-- and the command to run")
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("give the command after --")

    texts = []
    for path in args.message:
        with open(path, encoding="utf-8") as f:
            texts.append(f.read().strip())

    log = open(args.log, "w", encoding="utf-8")
    err = open(args.stderr, "w", encoding="utf-8")
    start = time.monotonic()
    proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=err, text=True, bufsize=1)
    lines = queue.Queue()
    threading.Thread(target=reader, args=(proc.stdout, lines), daemon=True).start()

    events = []
    turns = []
    ended = False

    def read_until_result(deadline):
        nonlocal ended
        while True:
            left = deadline - time.monotonic()
            if left <= 0:
                return None
            try:
                line = lines.get(timeout=left)
            except queue.Empty:
                return None
            if line is None:
                ended = True
                return None
            log.write(line)
            log.flush()
            try:
                event = json.loads(line)
            except ValueError:
                event = {"type": "not-json", "text": line[:200]}
            event["_at"] = round(time.monotonic() - start, 1)
            events.append(event)
            if event.get("type") == "result":
                return event

    for number, text in enumerate(texts, 1):
        sent = time.monotonic()
        proc.stdin.write(user_line(text))
        proc.stdin.flush()
        result = read_until_result(sent + args.turn_timeout)
        turns.append({
            "turn": number,
            "seconds": round(time.monotonic() - sent, 1),
            "result": result,
        })
        if result is None:
            break
    proc.stdin.close()
    if not ended:
        read_until_result(time.monotonic() + 120)
    try:
        code = proc.wait(timeout=120)
    except subprocess.TimeoutExpired:
        proc.terminate()
        code = proc.wait()
    log.close()
    err.close()

    # The secret check: a secret in the log is removed, and named.
    leaked = []
    with open(args.log, encoding="utf-8") as f:
        text = f.read()
    for name, value in secrets(args.secret_env, args.secret_file):
        if value in text:
            leaked.append(name)
            text = text.replace(value, "[removed]")
    if leaked:
        with open(args.log, "w", encoding="utf-8") as f:
            f.write(text)

    summary = summarize(events, turns, code, leaked, len(texts))
    with open(args.summary, "w", encoding="utf-8") as f:
        f.write(summary)
    sys.stdout.write(summary)
    ok = code == 0 and len(turns) == len(texts) and all(
        t["result"] and not t["result"].get("is_error") for t in turns) and not leaked
    sys.exit(0 if ok else 1)


def summarize(events, turns, code, leaked, wanted):
    out = []
    put = out.append
    put("## The stream-json events\n")
    put(f"- Exit code of the command: {code}.")
    put(f"- Turns: {len(turns)} of {wanted}.")
    put(f"- Secrets in the log: {', '.join(leaked) + ' (removed)' if leaked else 'none'}.")

    kinds = collections.Counter()
    for event in events:
        kind = event.get("type", "?")
        if event.get("subtype"):
            kind += "/" + event["subtype"]
        kinds[kind] += 1
    put("- Events: " + ", ".join(f"`{k}` {n}" for k, n in sorted(kinds.items())) + ".")

    inits = [e for e in events if e.get("type") == "system" and e.get("subtype") == "init"]
    sessions = sorted({e.get("session_id") for e in events if e.get("session_id")})
    put(f"- Session IDs in the events: {len(sessions)}.")
    for i, init in enumerate(inits, 1):
        put(f"\n### init {i} (at {init['_at']} s)\n")
        put("- Fields: " + ", ".join(f"`{k}`" for k in sorted(init) if k != "_at") + ".")
        for key in ("model", "permissionMode", "apiKeySource", "claude_code_version",
                    "output_style", "cwd"):
            if key in init:
                put(f"- `{key}`: `{init[key]}`")
        for key in ("tools", "mcp_servers", "slash_commands", "agents", "skills", "plugins"):
            if key in init:
                value = init[key]
                if isinstance(value, list):
                    names = [v.get("name", str(v)) if isinstance(v, dict) else str(v)
                             for v in value]
                    put(f"- `{key}` ({len(names)}): " + ", ".join(names[:40]))
                else:
                    put(f"- `{key}`: {value}")

    put("\n### Tool calls\n")
    calls = 0
    for event in events:
        if event.get("type") != "assistant":
            continue
        for block in event.get("message", {}).get("content", []) or []:
            if isinstance(block, dict) and block.get("type") == "tool_use":
                calls += 1
                detail = short_input(block.get("name"), block.get("input"))
                put(f"- {event['_at']} s: `{block.get('name')}` {detail}")
    errors = 0
    for event in events:
        if event.get("type") != "user":
            continue
        for block in event.get("message", {}).get("content", []) or []:
            if isinstance(block, dict) and block.get("type") == "tool_result" and block.get("is_error"):
                errors += 1
                content = block.get("content")
                if isinstance(content, list):
                    content = " ".join(c.get("text", "") for c in content if isinstance(c, dict))
                put(f"- tool error at {event['_at']} s: {str(content).splitlines()[0][:200] if content else ''}")
    put(f"\nTool calls: {calls}. Tool errors: {errors}.")

    others = [e for e in events if e.get("type") == "system" and e.get("subtype") != "init"]
    if others:
        put("\n### Other system events\n")
        for event in others:
            fields = ", ".join(sorted(k for k in event if k not in ("_at", "type")))
            put(f"- {event['_at']} s: `{event.get('subtype')}` ({fields})")

    for turn in turns:
        put(f"\n### The result of turn {turn['turn']} ({turn['seconds']} s)\n")
        result = turn["result"]
        if result is None:
            put("- No result: the turn timed out, or the command ended.")
            continue
        for key in ("subtype", "is_error", "num_turns", "duration_ms", "duration_api_ms",
                    "total_cost_usd", "stop_reason"):
            if key in result:
                put(f"- `{key}`: {result[key]}")
        if "usage" in result:
            put(f"- `usage`: `{json.dumps(result['usage'], sort_keys=True)}`")
        if "modelUsage" in result:
            put(f"- `modelUsage` models: {', '.join(sorted(result['modelUsage']))}")
        denials = result.get("permission_denials") or []
        put(f"- `permission_denials`: {len(denials)}")
        for denial in denials:
            put(f"  - `{denial.get('tool_name')}` {short_input(denial.get('tool_name'), denial.get('tool_input'))}")
        text = str(result.get("result", "")).strip().replace("\n", " ")
        put(f"- `result`: {text[:500]}")
    put("")
    return "\n".join(out)


if __name__ == "__main__":
    main()
