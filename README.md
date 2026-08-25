<div align="center">
  <img src="src/assets/app-icon.png" width="96" alt="Codex Switcher icon">
  <h1>Codex Switcher</h1>
  <p>A local-first desktop app for managing Codex OAuth and API Key accounts.</p>
  <p>
    <strong>English</strong> |
    <a href="README.zh-CN.md">简体中文</a> |
    <a href="README.zh-TW.md">繁體中文（台灣）</a> |
    <a href="README.ru.md">Русский</a>
  </p>
</div>

Codex Switcher keeps multiple Codex sign-in profiles in one desktop app. It can switch the active account, write the selected credentials back to the local Codex configuration, monitor quotas, restore sessions, and summarize local token usage and estimated cost.

## Screenshots

### Account overview

![Account overview with privacy mode enabled](doc/assets/screenshots/accounts-overview.jpg)

### Usage dashboard

![Local token usage and estimated cost dashboard](doc/assets/screenshots/usage-dashboard.jpg)

## Highlights

- **OAuth and API Key accounts**: add, edit, import, export, filter, sort, and switch between multiple accounts.
- **Independent Codex instances (macOS)**: launch multiple official desktop instances with separate `CODEX_HOME` and Electron data directories, then target account switching, OpenCodex sync/restore, and configuration editing to a selected instance.
- **Quota monitoring**: inspect account availability, subscription periods, standard quota windows, and optional GPT 5.3 Codex Spark quota windows.
- **Session tools**: manage, repair, back up, and restore local Codex sessions.
- **Usage analytics**: aggregate local token usage, cache activity, model distribution, and estimated cost with configurable pricing rules.
- **CLIProxyAPI integration**: install, update, run, configure, and bind selected accounts to the bundled API service.
- **Privacy controls**: mask account identifiers in the interface and keep application data on the local machine.
- **Four interface languages**: English, Simplified Chinese, Traditional Chinese (Taiwan), and Russian.

## Built-in CPA API service

The **API Service** page provides integrated management for the official [CLIProxyAPI (CPA)](https://github.com/router-for-me/CLIProxyAPI) runtime, so a separate manual deployment is not required.

- **Quick operations**: detect the current platform, download and install the matching package, then start, stop, update, or reset the service from the app.
- **Account binding**: send selected OAuth accounts to the CPA authentication directory and API Key accounts to its upstream configuration; bound accounts can also be reviewed and removed.
- **Service configuration**: manage the listening port, management key, access API keys, automatic updates, and update-check interval.
- **Version management**: detect official releases, import a trusted local package, inspect installed versions, switch the active version, and remove inactive versions.
- **Safer switching**: switching while CPA is running automatically restarts the service; if the selected version cannot start, Codex Switcher restores the previous version.
- **Local workspace**: runtimes, configuration, authentication files, and download cache are kept under `~/.codex_switcher/api-service`.

## Install

Download the latest installer from [GitHub Releases](https://github.com/vs2pk0/codex-switcher/releases):

- **Windows**: use the `.exe` or `.msi` installer.
- **macOS**: open the `.dmg` and move Codex Switcher to Applications, or run the `.pkg` installer.

If macOS blocks the first launch, open **System Settings > Privacy & Security** and allow the app to open.

### macOS says the app is damaged and cannot be opened

The current macOS package is not notarized, so Gatekeeper may quarantine it and report that the app is damaged. Only if the package was downloaded from this project's official [GitHub Releases](https://github.com/vs2pk0/codex-switcher/releases), move the app to `/Applications` and run:

```bash
sudo xattr -rd com.apple.quarantine "/Applications/Codex Switcher.app"
```

Then open Codex Switcher again. This command only removes the quarantine attribute from this application; do not use it on an app downloaded from an untrusted source.

## Basic workflow

1. Add an OAuth or API Key account from the account overview.
2. Refresh the account to retrieve its latest quota or balance.
3. Select an account and switch to it. Codex Switcher updates the local Codex authentication and configuration files.
4. Use **Sessions**, **Usage**, or **API Service** for session recovery, statistics, and CLIProxyAPI management.

### Multiple Codex desktop instances on macOS

Open **Codex Instances**, create an instance, and optionally choose its Codex home, Electron data directory, workspace, and official app bundle. Empty data paths are generated under `~/.codex_switcher/instances`. The launcher verifies that the installed official app supports isolated desktop data before starting it. Permanently deleting a managed instance first stops it, then removes its Codex home, Electron data, managed profile, session trash, configuration backups, and manually-created session backups that can be attributed to that instance. Workspaces, the official app, the system-default instance, and other instances are protected. Directory-boundary checks reject shared or overlapping data paths before deletion.

When more than one instance exists, switching an account and the OpenCodex sync/restore actions ask which instance to target. Sessions can be browsed and managed per instance, and the copy dialog can create an independent copy in another instance's session store. The Settings page also lets you select the instance whose Codex path and configuration files are displayed or edited.

Default/full backups include sessions from the official default instance only. Managed multi-open instance data is deliberately excluded and must be backed up manually after selecting that instance on the Sessions page.

## Local data and privacy

Application data is stored under `~/.codex_switcher` by default, including:

- Accounts: `~/.codex_switcher/account/accounts.json`
- Sessions: `~/.codex_switcher/session`
- Usage statistics: `~/.codex_switcher/statistics`
- Settings: `~/.codex_switcher/data/settings.json`
- Backups: `~/.codex_switcher/backup`

The active profile is written to the local Codex directory at `~/.codex`. Account exports may contain credentials; store exported files securely.

## Support the project

Thank you for supporting Codex Switcher. Donations help cover ongoing maintenance, feature development, test devices, and packaging and distribution costs. If the app saves you time, you can support the project through any of the methods below.

<table>
  <tr>
    <th>Alipay</th>
    <th>WeChat Pay</th>
    <th>Binance</th>
  </tr>
  <tr>
    <td align="center"><img src="doc/assets/sponsor/alipay.png" height="220" alt="Alipay donation QR code"></td>
    <td align="center"><img src="doc/assets/sponsor/wechat.png" height="220" alt="WeChat Pay donation QR code"></td>
    <td align="center"><img src="doc/assets/sponsor/binance.jpg" height="220" alt="Binance donation QR code"></td>
  </tr>
</table>

## Development

### Requirements

- Node.js 22
- Rust stable toolchain
- The [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform

### Run locally

```bash
npm ci
npm run tauri -- dev
```

### Validate and build

```bash
npm run typecheck
npm run build
npm run tauri -- build
```

The desktop application is built with Tauri 2, Vue 3, TypeScript, Rust, Arco Design, and ECharts.
