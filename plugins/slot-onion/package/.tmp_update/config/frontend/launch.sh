#!/bin/sh
log_dir=/mnt/SDCARD/.tmp_update/logs
mkdir -p "$log_dir"
exec >>"$log_dir/slot-onion.log" 2>&1
echo "--- Slot-Onion launch $(date '+%Y-%m-%d %H:%M:%S' 2>/dev/null || date) ---"
echo "launcher: $0"
echo "binary: /mnt/SDCARD/.tmp_update/bin/slot-onion"
audio_server_script=/mnt/SDCARD/.tmp_update/script/stop_audioserver.sh
if [ -f "$audio_server_script" ]; then
    echo "stopping Onion audioserver for raw Miyoo audio"
    . "$audio_server_script"
else
    echo "Onion stop_audioserver.sh not found; raw audio may be busy"
fi
exec /mnt/SDCARD/.tmp_update/bin/slot-onion --sdcard /mnt/SDCARD --tmp /tmp
