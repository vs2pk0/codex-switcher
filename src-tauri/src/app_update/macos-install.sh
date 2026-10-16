#!/bin/sh
# All paths arrive as argv, never as interpolated shell source.
set -u
trap '' HUP
pid="$1"
target="$2"
work="$3"
error_file="$4"
cleanup_marker="$5"
backup="$work/previous.app"
staged="$work/new.app"
exec >> "$work/install.log" 2>&1

report_failure() {
    /usr/bin/printf '%s\n日志：%s\n' "$1" "$work/install.log" > "$error_file"
}

rollback() {
    # Retain the failed bundle and log for diagnosis; never delete the only good copy.
    if [ -e "$target" ]; then
        /bin/mv "$target" "$work/failed.app" || return 1
    fi
    /bin/mv "$backup" "$target"
}

: > "$work/ready" || exit 1
attempt=0
while /bin/kill -0 "$pid" 2>/dev/null; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 600 ]; then
        report_failure '当前应用未能退出，已取消更新，旧版本保持不变。'
        exit 1
    fi
    /bin/sleep 0.2
done

if ! /bin/mv "$target" "$backup"; then
    report_failure '无法替换当前应用，旧版本保持不变，请检查安装目录权限。'
    /usr/bin/open -n "$target"
    exit 1
fi
if ! /bin/mv "$staged" "$target"; then
    if rollback; then
        report_failure '安装新版失败，已恢复旧版本。'
        /usr/bin/open -n "$target"
    else
        report_failure "安装失败且自动恢复失败，请将 $backup 移回 $target。"
    fi
    exit 1
fi

started=false
if /usr/bin/open -n "$target" --args --codex-switcher-update-stage "$work"; then
    attempt=0
    while [ "$attempt" -lt 150 ]; do
        if [ -f "$work/launch-confirmed" ]; then
            started=true
            break
        fi
        attempt=$((attempt + 1))
        /bin/sleep 0.2
    done
fi

if [ "$started" != true ]; then
    # Avoid moving a live but unresponsive new app: keep the backup for manual recovery.
    # Launch failure without a running app is safe to roll back.
    if /usr/bin/pgrep -x codex-switcher >/dev/null 2>&1; then
        report_failure "新版启动后未确认就绪，旧版本备份保留在 $backup。"
    elif rollback; then
        report_failure '新版启动失败，已恢复并重新打开旧版本。'
        /usr/bin/open -n "$target"
    else
        report_failure "新版启动失败且自动恢复失败，请将 $backup 移回 $target。"
    fi
    exit 1
fi

: > "$cleanup_marker"
/bin/rm -f "$error_file"
# Startup acknowledgement guarantees the new process no longer needs staging files.
/bin/rm -rf "$work"
