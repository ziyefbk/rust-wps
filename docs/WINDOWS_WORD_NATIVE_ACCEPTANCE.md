# Windows Word 原生有界验收

## 第三次托管运行及同提交复跑：单页原生闭环连续两次通过（2026-10-16）

[运行 38895615373](https://github.com/ziyefbk/rust-wps/actions/runs/38895615373)，[提交 93972c7af5a4b010f73b0823ef1bea94de40110d](https://github.com/ziyefbk/rust-wps/commit/93972c7af5a4b010f73b0823ef1bea94de40110d)，attempt 1 job `116955251837` 与未改产品代码的 attempt 2 job `116958800901`：两次全部步骤成功，原生 acceptance 步骤均包含清理且结果为 `success`，不是仅依据清理前写入的 report.json。下方准备与前两次失败章节保留各自当时状态；本节为最新的有界结果。

- Windows MSVC 的实际 Rust CLI 与 Tauri debug / native-smoke 构建通过；实际入口是生产 dist 的 `http://tauri.localhost/`，不是 Vite 模拟 IPC 页面。CLI 仅构建 / 记录散列，不代写本轮 Save 的 DOCX
- 两次均为 `initialReady=true`、`diagnosticReload=false`；实际新建处理器存在后首次点击进入编辑，没有用重载掩盖启动失败。首次通过的启动事件仅记录正常资源响应；两次通过均未依靠 reload，仍不能反推早先启动失败的根因或宣称一般稳定性
- 一页 `standard-v2` 探针：正文 `BodyProbe 中文`、三个英文标题与一个空尾段；实际键盘 / 样式下拉、CSS、真实 Rust 草稿后端、Save 按钮 / Tauri IPC 均通过。11 / 18 / 15 / 13 pt 默认字号与黑色保持，无显式字号或颜色标记
- 第二次 Save 拒绝已有目标，DOCX 字节不变且编辑器恢复；独立 python-docx 完整正文 / 标题 / 默认字体 / 字号、A4 / 四边 25.4 mm 检查通过；实际 Open / worker 重开保持正文与样式
- 原生 WebView2 PDF 输出与独立 pypdf 测量通过：一页约 594.960 × 841.920 pt，全文含中文，Arial / Microsoft YaHei 字体资源、受测 Latin 字号 / 黑色 / 左边界符合原阈值；打印不改草稿或 DOCX。没有放宽既有断言或作 Unicode 规范化

实际工具：Rust 1.88.0 / LLVM 20.1.5，Node 22.14.0，Python 3.12.10，python-docx 1.1.2，pypdf 6.10.0，WebView2 131.0.2903.86；runner 映像与字体文件散列见 job log。没有因 WebView2 版本推测缺失 API：该次探针显示 `Promise.try` 存在，而 Map `getOrInsertComputed` / Uint8Array `fromBase64` 缺失；本次 Word 路径通过不证明这些 PDF.js 后续路径可用。

attempt 1 证据身份：

- 桌面二进制 SHA-256：`e3ce39b3312e8f8174e08efe5cfc97e10aea9ded777e2e21f6c2382a3c1227f5`
- 根 CLI SHA-256：`32e9dda95c9edc429f7bf4f5f82fb807bcdbd3b738fe6ae53f607fe47e442258`
- 输出 DOCX SHA-256：`cb294ca3cf42081dac9364fb6795e58592b171b188f338dedbf311e50b9f2562`
- 保存的完整 job log SHA-256：`55bdd1a74b3ad7c5801da38d9444395b8d19fbee1e9cbf05df5e5fc0aa68043f`

同提交 attempt 2 的工具版本相同，实际 Save / 拒绝覆盖 / 独立读取 / 重开 / 原 PDF 阈值及清理再次通过；输出 DOCX SHA-256 与 attempt 1 相同。两次是同一场景的重复执行，不计为新增不同测试：

- attempt 2 桌面二进制 SHA-256：`b0354c9004ff93536037f9905d3fb95b6acd25366486089acc41517c7f29e5b6`
- attempt 2 根 CLI SHA-256：`63530f5235a9f9439cd41a093cb535aa6a2fe7effe7c146c68a6934dcfa7670c`
- attempt 2 完整 job log SHA-256：`a96ca8cb1654fca5b12bcf1728e401d6c3ec1af71df012933b5d7b528ad6b0d5`

两轮二进制散列不同，未建立可复现构建；保留各自真实身份，不把同一源码等同相同二进制。

接受范围仅为上述单页标准样式的原生闭环，不是完整回归、生产包或日常可用里程碑完成。路径选择仍为 smoke-only 覆盖，草稿及 WebView2 profile 是隔离目录；未验收系统文件 / 打印对话框、IME、剪贴板、默认 AppData 恢复、安装包、真实 Office 消费者或一般分页 / 字体替代保真。未上传 artifacts，因而没有可下载的本轮 DOCX / PDF / 截图，也未做独立视觉检查；日志中的文字 / 几何与文件散列不能替代目视。

本次结果更新只改文档，不改变 workflow、不触发额外 Windows 作业，不重复计算历史 Rust / 前端测试。


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
node node_modules/@tauri-apps/cli/tauri.js build --debug --no-bundle --features native-smoke -- --locked
$env:RUST_WPS_SMOKE_PYTHON = (Get-Command python).Source
node scripts/native-word-style-smoke.mjs
```

Tauri CLI 执行生产前端 `npm run build` 并启用 custom protocol；产物是 debug native-smoke 可执行文件，不是安装包或无测试钩子的生产发布。作业记录实际 source commit 和两个二进制散列。

公开仓库的标准托管 runner 按 GitHub 当前说明无需计算费用；本作业不使用 artifact / cache 存储。参见 [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions)。不改变仓库计费或 Actions 配置。

## 证据保留与未完成项

原始 DOCX / PDF / 截图在 runner 的 `tmp/native-word-style/run-*`，JSON 测量与进程日志输出到 job logs。没有 artifact 上传，因此 VM 结束后不能取得截图 / PDF 进行独立视觉验收；文件散列或文本日志不等于看过图像。

本轮本地 `node --check`、独立 `rustfmt --check`、YAML 解析 / 单作业 / timeout / SHA 约束与 `git diff --check` 通过；未运行 Windows、Cargo 编译、原生脚本或完整回归。

本地只允许轻量静态检查，不在资源紧张的云 Linux 上启动完整 Cargo / Tauri 构建。发布、workflow 写权限、托管 runner 可用性、实际构建与完整脚本结果必须逐项核验；遇到权限拒绝停止，不扩大凭据。如果严格 PDF 或原生 UI 门禁失败，保留失败和测量，不把门禁降级为通过。

## 首次托管运行失败与命令修复（2026-10-16）

[首次运行 38893427886](https://github.com/ziyefbk/rust-wps/actions/runs/38893427886) 的源码为 `7969e31f3221763bb379974f0c20318f53f55eaf`。Rust 1.88.0 的根 CLI 已实际构建成功；随后 Windows 上 `npm exec -- tauri build ... -- --locked` 未将分隔符保留至 Tauri，报 `unexpected argument '--locked' found`。Tauri 桌面构建与原生 smoke 均未执行，不能称 Save / Open / PDF 验收通过。

命令修复直接用 Node 调用 lockfile 安装的 `@tauri-apps/cli/tauri.js`，保留 `-- --locked`，不经 npm 的命令包装，不删除锁定要求。轻量本地验证调用实际 Tauri 2.11.5，覆盖 beforeBuild 为记录标记、runner 为只记录参数后退出 86 的脚本，观察到精确参数 `build --locked --bins --features native-smoke,tauri/custom-protocol`；没有运行 Cargo 编译。该验证只证明参数运输和 Tauri 构建钩子，不证明 Windows 构建；本地 Linux 的原 npm 命令也能正常运输，因此不能称本地复现 Windows 故障。修复后的托管运行仍需独立核验。

## 第二次托管运行：构建通过，初始页面未进入编辑（2026-10-16）

[运行 38894216388](https://github.com/ziyefbk/rust-wps/actions/runs/38894216388)，源码 `a6a87c7da74363f7916d0b2f4f08b7b920222799`：Rust CLI 与实际 Tauri debug/native-smoke 构建均成功，CDP 连接实际 WebView2 `131.0.2903.86`。点击新建按钮后 30 秒未出现 `.tiptap`，仍为初始“文档工作区 / 就绪”；Save、独立 DOCX、重开与 PDF 未执行。原进程日志为空，原 pageerror 监听可能错过连接前的启动异常。

静态 HTML 中新建按钮一开始可点击，而应用模块完成初始化才注册处理器，因此点击返回不能证明启动完成。源检查未证明某个 PDF.js 新 API 导致启动失败；其部分新 API 出现在后续 PDF 操作中，不能仅依据 WebView2 版本归因。

脚本现在先启用 CDP Log / Runtime 与 console / pageerror / requestfailed 诊断，记录真实页面 URL、脚本 / 资源、CSP meta、桥接及关键 API 支持状态；等待实际页面 DOMContentLoaded 后，用 CDP 检查真实新建 click 处理器，再进行原操作。初次启动未绑定处理器时，仅在尚未编辑前对同一真实应用地址做一次带预加载诊断监听的 reload，记录异常 / CSP 与是否恢复；即使 reload 恢复也仍判原始启动门禁失败，不转为接受。未加 polyfill、模拟 Tauri、绕过 CSP、放宽文字或 PDF 断言，也没有延长原编辑超时。新增诊断只改善定位，本轮原生功能结果仍待托管重跑。
