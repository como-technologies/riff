# Reads the settings of one riff in the cloud (01M3ZE3Z580RB5AYAJX6321DFW).
# Each cloud script sources this file before it reads its arguments.
#
# The first argument of the script can name the settings: `stage`
# reads deploy/stage.env, and the name goes from the arguments. With no
# name, or the name `cloud`, the script reads deploy/cloud.env: the
# settings of the shared riff. A name starts with a lowercase letter
# and has only lowercase letters, digits and dashes, so a tag, a number
# or an option is no name.
#
# It sets CLOUD_NAME (empty for the shared riff), CLOUD_SETTINGS (the
# path of the file) and each setting of the file.
cloud_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
CLOUD_NAME=
if [[ ${1:-} =~ ^[a-z][a-z0-9-]*$ ]]; then
    if [ "$1" != cloud ]; then
        CLOUD_NAME=$1
    fi
    shift
fi
CLOUD_SETTINGS=$cloud_dir/${CLOUD_NAME:-cloud}.env
if [ ! -f "$CLOUD_SETTINGS" ]; then
    echo "riff has no cloud settings $CLOUD_NAME: deploy/$CLOUD_NAME.env is not there." >&2
    exit 1
fi
. "$CLOUD_SETTINGS"
