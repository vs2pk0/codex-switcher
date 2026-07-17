<div align="center">
  <img src="src/assets/app-icon.png" width="96" alt="Codex Switcher 图标">
  <h1>Codex Switcher</h1>
  <p>本地优先的 Codex OAuth 与 API Key 多账号管理桌面应用。</p>
  <p>
    <a href="README.md">English</a> |
    <strong>简体中文</strong> |
    <a href="README.zh-TW.md">繁體中文（台灣）</a> |
    <a href="README.ru.md">Русский</a>
  </p>
</div>

Codex Switcher 将多个 Codex 登录配置集中在一个桌面应用中。它可以切换当前账号、将选中的认证信息写回本机 Codex 配置，并提供额度监控、会话恢复、本地 Token 用量统计和费用预估。

## 应用截图

### 账号总览

![开启隐私模式的账号总览](doc/assets/screenshots/accounts-overview.jpg)

### 消费看板

![本地 Token 用量与预估费用看板](doc/assets/screenshots/usage-dashboard.jpg)

## 主要功能

- **OAuth 与 API Key 账号**：添加、编辑、导入、导出、筛选、排序和切换多个账号。
- **额度监控**：查看账号可用状态、订阅有效期、标准额度窗口，以及可选的 GPT 5.3 Codex Spark 额度窗口。
- **会话工具**：管理、修复、备份和恢复本机 Codex 会话。
- **使用统计**：汇总本地 Token、缓存、模型分布和预估费用，并支持自定义模型计价规则。
- **CLIProxyAPI 集成**：安装、更新、运行和配置内置 API 服务，并将选中的账号绑定到服务。
- **隐私控制**：在界面中隐藏账号标识，应用数据默认保存在本机。
- **四种界面语言**：English、简体中文、繁體中文（台灣）和 Русский。

## 内置 CPA API 服务

“API 服务”页面基于官方 [CLIProxyAPI（CPA）](https://github.com/router-for-me/CLIProxyAPI) 运行时提供一体化管理，不需要另外手动部署服务。

- **快捷运维**：自动识别当前平台并下载匹配的安装包，可直接在应用内安装、启动、停止、更新或重置服务。
- **账号绑定**：将选中的 OAuth 账号写入 CPA 认证目录，将 API Key 账号写入上游配置，并可查看或删除已经绑定的账号。
- **服务配置**：统一管理监听端口、管理密钥、访问 API Key、自动更新和更新检测间隔。
- **版本管理**：检测官方版本、导入可信的本地安装包、查看已安装版本、切换当前版本和删除非当前版本。
- **安全切换**：CPA 运行中切换版本会自动重启；如果新版本启动失败，Codex Switcher 会恢复到原来的版本。
- **本地工作区**：运行时、配置、认证文件和下载缓存统一保存在 `~/.codex_switcher/api-service`。

## 安装

从 [GitHub Releases](https://github.com/vs2pk0/codex-switcher/releases) 下载最新安装包：

- **Windows**：使用 `.exe` 或 `.msi` 安装包。
- **macOS**：打开 `.dmg` 并将 Codex Switcher 移入“应用程序”，或直接运行 `.pkg` 安装包。

如果 macOS 首次启动时阻止应用运行，请前往 **系统设置 > 隐私与安全性**，允许打开该应用。

### macOS 提示“应用已损坏，无法打开”？

当前 macOS 安装包尚未经过公证，Gatekeeper 可能会隔离应用并提示文件已损坏。请先确认安装包来自本项目官方 [GitHub Releases](https://github.com/vs2pk0/codex-switcher/releases)，然后将应用移入 `/Applications` 并执行：

```bash
sudo xattr -rd com.apple.quarantine "/Applications/Codex Switcher.app"
```

执行后重新打开 Codex Switcher。该命令只会移除这个应用的隔离属性；不要对来源不可信的应用执行此命令。

## 基本使用流程

1. 在账号总览中添加 OAuth 或 API Key 账号。
2. 手动刷新账号，读取最新额度或余额。
3. 选择账号并切换，Codex Switcher 会更新本机 Codex 的认证和配置文件。
4. 通过“会话管理”“使用统计”或“API 服务”进行会话恢复、统计分析和 CLIProxyAPI 管理。

## 本地数据与隐私

应用数据默认存放在 `~/.codex_switcher`，主要包括：

- 账号：`~/.codex_switcher/account/accounts.json`
- 会话：`~/.codex_switcher/session`
- 使用统计：`~/.codex_switcher/statistics`
- 设置：`~/.codex_switcher/data/settings.json`
- 备份：`~/.codex_switcher/backup`

当前账号会写入本机 Codex 目录 `~/.codex`。账号导出文件可能包含认证信息，请妥善保管。

## 赞赏支持

感谢你愿意支持 Codex Switcher。赞赏会用于持续维护、功能开发、测试设备和打包分发成本。如果这个工具帮你节省了时间，欢迎选择下面任意方式支持项目。

<table>
  <tr>
    <th>支付宝</th>
    <th>微信</th>
    <th>Binance</th>
  </tr>
  <tr>
    <td align="center"><img src="doc/assets/sponsor/alipay.png" height="220" alt="支付宝赞赏码"></td>
    <td align="center"><img src="doc/assets/sponsor/wechat.png" height="220" alt="微信赞赏码"></td>
    <td align="center"><img src="doc/assets/sponsor/binance.jpg" height="220" alt="Binance 收款码"></td>
  </tr>
</table>

## 本地开发

### 环境要求

- Node.js 22
- Rust stable 工具链
- 当前平台对应的 [Tauri 2 系统依赖](https://v2.tauri.app/zh-cn/start/prerequisites/)

### 启动开发环境

```bash
npm ci
npm run tauri -- dev
```

### 检查与构建

```bash
npm run typecheck
npm run build
npm run tauri -- build
```

桌面端基于 Tauri 2、Vue 3、TypeScript、Rust、Arco Design 和 ECharts 构建。
