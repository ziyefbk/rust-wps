# Windows Word 原生有界验收

## 状态与边界

本轮准备可运行的 Windows 原生门禁；本地 Linux 环境只做静态检查，尚无本轮 Windows 运行结果，不能称原生通过。既有发布 44 / 43 的结果与失败记录不变。

目标链路为实际 Windows WebView2 编辑器 → 实际 Save 按钮与 Tauri IPC → 生产 DOCX 导出 → 独立 python-docx 检查 → 实际 Open / worker 重开 → WebView2 原生 PDF 输出及 pypdf 测量。不是 CLI 代写 DOCX，也不使用浏览器 Tauri 模拟运输。

`native-smoke` 编译特性下新增 `RUST_WPS_SMOKE_DOCX_OUTPUT`，沿用 XLSX / PPTX 输出覆盖模式；普通构建完全忽略该变量，未改变 `validate`、`export_docx`、原子发布与拒绝覆盖。文件选择仍由测试路径代替；草稿写真实 Rust 后端，但目录是隔离的测试目录，不是默认 AppData 恢复验收。

范围只覆盖一页标准样式探针，包括中文正文和三个英文标题。键盘插入中文不是 IME；不声明系统文件 / 打印对话框、剪贴板、安装包、真实 Office 消费者、一般分页 / 字体替代或完整保真通过。

## 门禁

`scripts/native-word-style-smoke.mjs`：

1. 要求 Windows，记录源码 commit、Rust / Node / Python / python-docx / pypdf / 实际 WebView2 版本及 CLI / 桌面二进制 SHA-256
2. 新建独立目录与 `WEBVIEW2_USER_DATA_FOLDER`；仅监听 loopback CDP，20 秒内要求原生 WebView2 可连接，失败明确终止
3. 通过实际键盘 / 标题下拉框编辑，检查实际 CSS 与持久草稿内容
4. 点击实际 `#edit-save`，等待成功及恢复可编辑；第二次点击同一路径必须返回现有目标拒绝，DOCX SHA-256 不变、编辑区恢复
5. 独立 python-docx 读取全文、标题、默认字体 / 字号 / 黑色、A4 纸张 / 四边距；额外检查嵌入模型版本，不以嵌入模型替代独立正文读取
6. 点击实际 Open，检查重开正文 / 样式与源文件不变，再通过原生 PDF 输出检查单页物理尺寸、文字、字体资源、字号、黑色和正文左边界；不放宽旧阈值，也不规范化中文掩盖差异
7. 打印不得改变草稿或 DOCX；成功 / 失败都保留测量日志。清理仅对本次启动的 PID 树执行 `taskkill /T /F`，有超时，不按进程名称杀死其他应用

Python 通过 `RUST_WPS_SMOKE_PYTHON` 明确指定，未指定时使用 PATH 中的 `python`；不依赖 Codex 私有安装路径。

## 单一托管 Windows 作业

`.github/workflows/windows-word-native.yml` 只有一个 `windows-2022` 标准作业，最长 60 分钟，其中 smoke 步最长 10 分钟。只在 main 分支修改该 workflow 文件时触发，以及手动 `workflow_dispatch`；普通代码推送不会启动新的广泛 CI。没有 matrix、缓存、artifact 上传、更大 runner、部署、自定义 secrets 或安全设置变更；contents 权限只读，checkout 不保存凭据。

三个 Actions 固定到官方 release 对应的完整 commit SHA（checkout v4.2.2、setup-node v4.4.0、setup-python v5.6.0）；job summary 记录实际 smoke 步骤结果（包括 cleanup），不能凭提前产生的 report.json 宣称通过。CLI 仅构建与记录散列，不参与本脚本的 Save / Open 执行。

固定 Rust 1.88.0（项目 MSRV）、Node 22.14.0、Python 3.12.10；`npm ci` 与两份 Cargo lock 保持锁定。独立读取器固定 python-docx 1.1.2 / pypdf 6.10.0 / lxml 5.3.2 / typing_extensions 4.13.2。runner / Evergreen WebView2 / 系统字体会变化，因此记录实际版本与字体文件散列，不把它们称为完全固定的渲染环境。没有自动安装 WebView2 的回退；缺失时明确失败。

实际构建命令：

```powershell
cargo build --locked --bin rust-wps
npm exec -- tauri build --debug --no-bundle --features native-smoke -- --locked
$env:RUST_WPS_SMOKE_PYTHON = (Get-Command python).Source
node scripts/native-word-style-smoke.mjs
```

Tauri CLI 执行生产前端 `npm run build` 并启用 custom protocol；产物是 debug native-smoke 可执行文件，不是安装包或无测试钩子的生产发布。作业记录实际 source commit 和两个二进制散列。

公开仓库的标准托管 runner 按 GitHub 当前说明无需计算费用；本作业不使用 artifact / cache 存储。参见 [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions)。不改变仓库计费或 Actions 配置。

## 证据保留与未完成项

原始 DOCX / PDF / 截图在 runner 的 `tmp/native-word-style/run-*`，JSON 测量与进程日志输出到 job logs。没有 artifact 上传，因此 VM 结束后不能取得截图 / PDF 进行独立视觉验收；文件散列或文本日志不等于看过图像。

本轮本地 `node --check`、独立 `rustfmt --check`、YAML 解析 / 单作业 / timeout / SHA 约束与 `git diff --check` 通过；未运行 Windows、Cargo 编译、原生脚本或完整回归。

本地只允许轻量静态检查，不在资源紧张的云 Linux 上启动完整 Cargo / Tauri 构建。发布、workflow 写权限、托管 runner 可用性、实际构建与完整脚本结果必须逐项核验；遇到权限拒绝停止，不扩大凭据。如果严格 PDF 或原生 UI 门禁失败，保留失败和测量，不把门禁降级为通过。
