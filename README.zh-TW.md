<div align="center">
  <img src="src/assets/app-icon.png" width="96" alt="Codex Switcher 圖示">
  <h1>Codex Switcher</h1>
  <p>本機優先的 Codex OAuth 與 API Key 多帳號管理桌面應用程式。</p>
  <p>
    <a href="README.md">English</a> |
    <a href="README.zh-CN.md">简体中文</a> |
    <strong>繁體中文（台灣）</strong> |
    <a href="README.ru.md">Русский</a>
  </p>
</div>

Codex Switcher 將多個 Codex 登入設定集中在一個桌面應用程式中。它可以切換目前帳號、將選取的驗證資訊寫回本機 Codex 設定，並提供額度監控、工作階段復原、本機 Token 用量統計與費用預估。

## 應用程式畫面

### 帳號總覽

![已開啟隱私模式的帳號總覽](doc/assets/screenshots/accounts-overview.jpg)

### 用量儀表板

![本機 Token 用量與預估費用儀表板](doc/assets/screenshots/usage-dashboard.jpg)

## 主要功能

- **OAuth 與 API Key 帳號**：新增、編輯、匯入、匯出、篩選、排序及切換多個帳號。
- **額度監控**：檢視帳號可用狀態、訂閱有效期限、標準額度週期，以及可選的 GPT 5.3 Codex Spark 額度週期。
- **工作階段工具**：管理、修復、備份與復原本機 Codex 工作階段。
- **使用統計**：彙整本機 Token、快取、模型分布及預估費用，並支援自訂模型計價規則。
- **CLIProxyAPI 整合**：安裝、更新、執行及設定內建 API 服務，並將選取的帳號綁定至服務。
- **隱私控制**：在介面中隱藏帳號識別資訊，應用程式資料預設保存在本機。
- **四種介面語言**：English、简体中文、繁體中文（台灣）與 Русский。

## 內建 CPA API 服務

「API 服務」頁面以官方 [CLIProxyAPI（CPA）](https://github.com/router-for-me/CLIProxyAPI) 執行環境提供整合式管理，不需要另外手動部署服務。

- **快速維運**：自動辨識目前平台並下載相符的安裝套件，可直接在應用程式內安裝、啟動、停止、更新或重設服務。
- **帳號綁定**：將選取的 OAuth 帳號寫入 CPA 驗證目錄，將 API Key 帳號寫入上游設定，並可檢視或刪除已綁定的帳號。
- **服務設定**：集中管理監聽連接埠、管理金鑰、存取 API Key、自動更新與更新檢查間隔。
- **版本管理**：檢查官方版本、匯入可信的本機安裝套件、檢視已安裝版本、切換目前版本及刪除非目前版本。
- **安全切換**：CPA 執行中切換版本時會自動重新啟動；若新版本無法啟動，Codex Switcher 會還原原本的版本。
- **本機工作區**：執行環境、設定、驗證檔案及下載快取統一保存在 `~/.codex_switcher/api-service`。

## 安裝

從 [GitHub Releases](https://github.com/vs2pk0/codex-switcher/releases) 下載最新安裝檔：

- **Windows**：使用 `.exe` 或 `.msi` 安裝檔。
- **macOS**：開啟 `.dmg` 並將 Codex Switcher 移至「應用程式」，或直接執行 `.pkg` 安裝檔。

若 macOS 在首次啟動時阻擋應用程式，請前往 **系統設定 > 隱私權與安全性**，允許開啟此應用程式。

### macOS 顯示「應用程式已損毀，無法打開」？

目前的 macOS 安裝套件尚未經過公證，Gatekeeper 可能會隔離應用程式並顯示檔案已損毀。請先確認安裝套件來自本專案官方 [GitHub Releases](https://github.com/vs2pk0/codex-switcher/releases)，接著將應用程式移至 `/Applications` 並執行：

```bash
sudo xattr -rd com.apple.quarantine "/Applications/Codex Switcher.app"
```

執行後重新開啟 Codex Switcher。此命令只會移除這個應用程式的隔離屬性；請勿對來源不可信的應用程式執行此命令。

## 基本使用流程

1. 在帳號總覽中新增 OAuth 或 API Key 帳號。
2. 手動重新整理帳號，讀取最新額度或餘額。
3. 選取帳號並切換，Codex Switcher 會更新本機 Codex 的驗證與設定檔案。
4. 透過「工作階段管理」、「使用統計」或「API 服務」進行工作階段復原、統計分析與 CLIProxyAPI 管理。

## 本機資料與隱私

應用程式資料預設存放於 `~/.codex_switcher`，主要包括：

- 帳號：`~/.codex_switcher/account/accounts.json`
- 工作階段：`~/.codex_switcher/session`
- 使用統計：`~/.codex_switcher/statistics`
- 設定：`~/.codex_switcher/data/settings.json`
- 備份：`~/.codex_switcher/backup`

目前帳號會寫入本機 Codex 目錄 `~/.codex`。帳號匯出檔案可能包含驗證資訊，請妥善保管。

## 贊賞支持

感謝你願意支持 Codex Switcher。贊賞會用於持續維護、功能開發、測試裝置與打包發佈成本。如果這個工具幫你節省了時間，歡迎選擇下列任一方式支持專案。

<table>
  <tr>
    <th>支付寶</th>
    <th>微信</th>
    <th>Binance</th>
  </tr>
  <tr>
    <td align="center"><img src="doc/assets/sponsor/alipay.png" height="220" alt="支付寶贊賞碼"></td>
    <td align="center"><img src="doc/assets/sponsor/wechat.png" height="220" alt="微信贊賞碼"></td>
    <td align="center"><img src="doc/assets/sponsor/binance.jpg" height="220" alt="Binance 收款碼"></td>
  </tr>
</table>

## 本機開發

### 環境需求

- Node.js 22
- Rust stable 工具鏈
- 目前平台所需的 [Tauri 2 系統相依套件](https://v2.tauri.app/start/prerequisites/)

### 啟動開發環境

```bash
npm ci
npm run tauri -- dev
```

### 檢查與建置

```bash
npm run typecheck
npm run build
npm run tauri -- build
```

桌面端使用 Tauri 2、Vue 3、TypeScript、Rust、Arco Design 與 ECharts 建置。
