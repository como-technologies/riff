//! Gives the code the target triple of the build in `RIFF_TARGET`: the
//! test runner of a worker goes in `CARGO_TARGET_<TRIPLE>_RUNNER`
//! (01M3ZGZMNH1YM56GYNYBMH7AWM).

fn main() {
    let target = std::env::var("TARGET").expect("cargo sets TARGET");
    println!("cargo:rustc-env=RIFF_TARGET={target}");
}
