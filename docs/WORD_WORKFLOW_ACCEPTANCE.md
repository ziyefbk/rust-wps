# 文字工作流浏览器验收

发布 43，2026-10-16。此轮聚焦日常文字操作的实际交互与文件闭环，并修正保存期间的输入状态。基线为发布 42 `1e3df8d`。下列结果只覆盖明确列出的样本、操作和运行层；原生桌面验收与阶段 1 仍开放。

> 当前结论：9 组实际浏览器工作流和 6 份输出 DOCX 已独立接受；3 份 PDF 中 2 份中文全文严格比较失败，整体门禁仍为 FAIL。826 项既有 headless 回归及 8 项保存锁真实浏览器回归均通过，共 834 个不同检查；9 组工作流另计。TypeScript / build 通过。

## 运行了哪一层

- 真实 headless Chromium 154.0.8037.57 执行实际应用 UI 与 TipTap，通过实际按钮、键盘输入和界面确认操作；Vite 对源码作开发转换，不是 `dist` 生产包运行
- Tauri 运输和打开 / 另存路径选择由明确标注的测试适配器提供；DOCX 检查、导入、导出及可信重开调用冻结的发布 42 production CLI
- 草稿写入隔离的磁盘 JSON，关闭浏览器后由新 Chromium 进程恢复。它覆盖前端恢复流程，不覆盖 Rust 草稿后端的验证、原子写入和桌面退出
- “打印”使用实际应用打印快照与 Chromium `page.pdf`；`window.print` 只记录调用，不启动系统打印对话框。它不代表 Windows 直接 PDF 导出或物理打印
- 中文通过 Playwright `insertText` 输入，不是实际中文输入法组合事件。模拟 host close、文件选择取消和故障注入均须按适配器层解读

普通 shell 的两次 Chromium 启动受 `socket() EPERM` 阻断，原失败保留。之后在受支持的云电脑 Terminal 运行相同 checkout、脚本和冻结 CLI，真实 Chromium 已启动并执行流程；不能据最初 shell 失败写成“浏览器不可用”。本轮另有下述保存生命周期的有界修正，不安装原生依赖。

## 样本与独立预期

1. 中文会议纪要：[独立作者夹具](../tests/fixtures/word-workflow/README.md)，由 `python-docx` 编写的三段 A4 测试文件，预先固定文字、Heading1 / Normal 样式和四边 72 pt 页边距。没有使用产品模型、导入器或导出器生成来源；这是作者测试夹具，不是真实第三方中文文件
2. Apache POI `Bug60337.docx`：仓库既有、未改造的英文 3×5 表格原件。使用现有 [来源与许可说明](../tests/fixtures/docx-cell-no-fill/README.md)，保留 LICENSE / NOTICE；编辑后在其中一格加入中文
3. 新建中文文档：从实际“新建”界面输入正文，使用加粗按钮及手动分页，再导出、重开并验证两页浏览器打印副本。这是本轮操作产物，不计为外部真实原件

DOCX 独立检查使用 `python-docx` 与 ZIP/XML 直接读取标准内容，核对全文、段落 / 表格、样式、加粗、分页和已指定的页面属性；不会把产品内嵌模型当成内容预期。可信模型重开是另一条产品完整性检查。PDF 同时保留 pypdf 和 Poppler 的原始提取结果，只移除布局空白后比较全部 Unicode 字符、顺序和出现次数，不作 NFKC 或字符替换。

## 验收矩阵

| 范围 | 本轮结果 | 接受边界 |
| --- | --- | --- |
| 中文外部夹具打开、导入审阅取消 / 接受、修改、撤销 / 重做、DOCX 保存、新浏览器重开 | PASS | 前端实际交互 + CLI；来源是作者夹具 |
| 打开取消 / 损坏文件、保存取消 / 不覆盖拒绝后的旧文档保护 | PASS | 文件选择和运输由测试适配器提供 |
| 持久测试草稿、失败阻止离开、重试与新进程恢复 | PASS | 不代表生产 Rust 草稿后端 |
| 延迟保存的显式只读锁、快照、重复保存 / 模拟关闭保护与结束后恢复编辑 | PASS，成功 / 取消 / 不覆盖错误分别执行 | 不支持保存期间并发输入；不代表真实 Tauri 窗口关闭 |
| 真实 Bug60337 表格修改、撤销 / 重做、保存 / 重开 | PASS | 一个英文外部表格，新增中文文字 |
| 新建中文、加粗、手动分页、保存 / 重开和不可变打印快照 | PASS | 受测两页文档，非 Word 通用分页 |
| PDF 全文双读取器严格一致 | FAIL，1/3 通过 | 表格通过；两份中文文档 pypdf Unicode 不一致，不豁免 |
| PDF 可视页面、Poppler 全文与分页面标记 | 3 份 / 4 页已复核；Poppler 3/3 通过 | 不代替上行严格失败或 Office 版式保真 |
| 保存锁专门回归：结束状态、其他锁与旧会话 | 8/8 PASS | 含直接模块控制；与上方操作流程分开计数 |
| 真实 Windows / Tauri IPC、系统文件 / 打印对话框、IME、剪贴板 | 未运行 | 原生日常可用验收仍开放 |
| TypeScript / production build | PASS | 保留既有 bundle-size 警告；不等于生产包交互验收 |
| 旧 headless 前端回归 | 826/826 PASS，零跳过 / 重试 / 错误 | 加 8 项真实浏览器专门回归，共 834 个不同检查；9 组工作流另计 |
| 当前完整 Rust / 后端回归 | 本轮未运行 | 发布 42 与更早记录仅为历史证据 |

最终工作流 `run-yMJlKi` 完成 9 组，包括三种延迟保存结果及直接 PDF 不支持时的状态保护。独立读取 6 份输出 DOCX 全部通过；3 份 PDF 共 4 页均已目视，文字可读、未见裁剪或方框字，两页文档的显式分页位置正确。39 次外部调用包括 26 次 CLI、10 次独立 Python 检查及 3 次光栅化，另有 37 次测试运输调用；这些不是另外 76 个测试。来源、预期和冻结 CLI 前后散列不变。

## 保存期间的输入状态修正

实际浏览器验收暴露：保存时编辑器仍显示 `contenteditable=true`，但既有事务保护拒绝文档变更，尝试输入的文字会无声丢弃。原失败与独立诊断保留。最小修正将保存中的编辑器明确设为只读并显示状态，在成功、取消或失败结束后，同一会话且无其他锁时恢复编辑；保留既有快照与事务保护。

本轮接受合同是“显式锁定 → 保存结束 → 恢复输入”，不宣称允许保存期间并发编辑。最终浏览器分别覆盖成功、取消和真实 CLI 不覆盖拒绝：提示与编辑区 / 工具栏锁定可见，重复 Save / New / Open 和模拟关闭被阻止，快照 / 草稿不变，结束后可继续输入。另 8 项真实 Chromium 专门回归通过，覆盖三种结束状态、独立 pause 锁与旧保存完成后的会话隔离。旧会话测试只证明旧 Save 不改变一个已暂停的新会话的内容、提示或锁，不证明任意直接调用 `openAuthored` 后都能自动恢复；实际 UI 的 Open / New 在保存期间仍被阻止。

## 已确认的中文 PDF 限制

会议纪要 PDF 的字形 `ToUnicode` 包含康熙部首码点，9 个位置另有正确原字的 `ActualText`。Poppler 使用这部分信息，完整文字 / 顺序 / 次数与独立预期一致；pypdf 6.10.0 的提取仍给出部首码点，因此严格双读取器检查失败。独立目视的该页可读，未见裁剪或方框字。

冻结的发布 42 CLI `inspect` 经 `pdf-extract 0.12.1` 也提取到部首码点。独立 Node 诊断另调用未修改的生产 `extractPdfText` / `findReadingMatches` 与 PDF.js 5.6.205，仍有部首和部分字体片段间空白；`项目`、`文档`、`月`、`日`、`人`、`林舟`、`行动` 的字面查找均为零。该诊断不是浏览器 PDF 界面验收，但说明 PDF.js 重提取没有修复本样本。限制影响产品的中文正文提取与精确查找，不能只归为测试读取器问题。正确的 `ActualText` 和可读页面不使严格失败变为通过；不改预期文字、不做 NFKC，也不以 Poppler 单读成功宣称一般 PDF 文本兼容。

## 复现入口

需要现有 Node/npm 依赖、可运行的 Chromium、Python `python-docx` / `pypdf`、Poppler `pdftotext` / `pdftoppm`，以及平台匹配的 CLI。脚本不自动安装或下载。每次运行在 `tmp/browser-word-workflow/run-*` 创建新的证据目录；失败报告、原始 stdout/stderr、DOCX、PDF 和截图保留，不覆盖来源或已有输出。

固定来源、预期和生成器已保存在 `tests/fixtures/word-workflow`。以下命令可使用调用者选定的平台匹配 CLI；本轮没有实际执行干净 checkout 或 fresh-host 资格验证。

```sh
node scripts/browser-word-workflow.mjs \
  --cli /absolute/path/to/platform-compatible-cli \
  --cli-sha256 VERIFIED_CLI_SHA256 \
  --source tests/fixtures/word-workflow/independent-chinese-minutes.docx \
  --expected tests/fixtures/word-workflow/independent-chinese-minutes.expected.json \
  --browser /usr/bin/chromium
```

实际云 Terminal 调用先将已安装的 Node / Python 运行时加入 PATH；完整调用日志保留。固定 CLI 是 `tmp/cold-validation-20261016/candidate-v1/stable/candidate-cli`，SHA-256 `d062da54e83352cec4a19212678904c8a53f06d8ae49dce8b42fd698f5018a63`。

保存锁的专门浏览器测试入口：

```sh
node node_modules/@playwright/test/cli.js test \
  --config tests/ui/word-save-readonly.config.mjs
```

该配置通过 `CHROMIUM_PATH` 选择其他浏览器路径，默认 `/usr/bin/chromium`；工作流脚本使用 `--browser`。专门测试中的旧会话和其他锁控制包含直接模块调用，不能全算作用户界面交互。

`--cli-sha256` 是调用者明确选择的 CLI 身份，不是工具自动判断任意 CLI 已验收。更换 CLI 后须使用其实际散列并重新运行，不能将此轮结果移植给其他二进制。`report.json` 的 `functionalFlowsPassed` 与整体 `passed` 分开：流程通过而严格 PDF 失败时，整体仍为 `false`，进程退出 1。

## 证据入口与保留失败

- 预先固定合同 / 来源：`tmp/release43-independent/acceptance-contract.json`、`independent-chinese-minutes.expected.json`、`fixture-preflight.json`
- 初次已执行浏览器的部分复核：`tmp/release43-independent/run-1poXct-partial-review.json`
- 中文 PDF 诊断：`tmp/release43-independent/minutes-pdf-unicode-diagnosis.json`、`minutes-pdf-product-inspect.stdout.json`、`run-yMJlKi-pdfjs-extraction.json`
- 保存期间输入丢弃的独立诊断：`tmp/release43-independent/delayed-save-input-diagnosis.json`
- 最终完整运行：`tmp/browser-word-workflow/run-yMJlKi/report.json`，实际命令日志 `tmp/browser-word-workflow/terminal-invocation-v8.log`
- 独立最终接受及源码桥：`tmp/release43-independent/FINAL_INDEPENDENT_REVIEW.json`
- 独立文件 / 读取器复核：`tmp/release43-independent/run-yMJlKi-artifact-review.json`
- 826 项既有 headless 回归：`tmp/release43-frontend/attempt-1tpuq8lg/report.json`
- 保存锁专门回归：`tmp/word-save-readonly/run-jypaOM/report.json`；TypeScript / build：`tmp/browser-word-workflow/final-build.log`
- 简明最终收据与源码身份：`tmp/browser-word-workflow/final-validation-receipt.json`；工作流后只调整专门测试的模块 URL / 输出路径，应用 UI 和工作流脚本未变

预先合同曾写“built production UI”，实际运行明确收窄为 Vite 转换的应用源码，生产 `dist` 交互验收未完成；production build 已通过。早期启动失败、首次遇到严格 PDF 失败即停止、后续光标定位失败和保存期间输入无声丢弃均保留，不能改写为完整流程通过；只以最终实际运行覆盖的范围作结论。

## 下一步验收

下一项已定位修复是 PDF `ActualText`：后端须有界保留正确原文，前端 PDF.js 重提取也要避免覆盖已经正确的文本；本轮仅完成诊断，没有实施这项修复。仍需补 Windows 原生窗口、真实 IPC / 文件与打印对话框、生产草稿写入与恢复、实际中文输入法，并用有许可的真实中文文档完成打开→修改→保存→重开→打印闭环。当前 Linux 缺开发包、WebKitGTK / JavaScriptCore 和显示 / 自动化前提；只做环境评估，未安装或构建。即使补齐 Linux 环境，代码仍明确不支持 Linux 直接 PDF 导出，不能替代 Windows 验收。
