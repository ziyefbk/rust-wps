# PDF 正文空白兼容查找

发布 41（2026-10-15）：有界 PDF 正文空白兼容查找的前端与独立验证已通过；范围与限制如下。

## 使用与匹配边界

PDF 正文模式新增独立的“空白兼容匹配”选项，默认关闭，可单独使用或与既有“连字兼容匹配”组合。仅影响只读正文查找，不改 PDF 源文件、提取结果、页面图像或编辑替换。

源文字与查询对称投影：

- 连续 ASCII 空格（U+0020）和 Tab（U+0009）折为一个空格
- 连续 ASCII 空白段最多包含一个逻辑换行时也折为一个空格；CRLF 算一次，单独 CR / LF 各算一次
- 含两个及以上逻辑换行的段、空行以及含 form feed（U+000C）的段保留为屏障，不允许兼容短语跨越；查询自身含屏障时不返回匹配
- 纯空白查询在此选项下不返回匹配；NBSP 等其他 Unicode 空白不折叠，不启用一般 Unicode 规范化
- 仍区分大小写，不删除连字符、不推断缺字，不做 OCR、列排序或阅读顺序重建

折叠只用于匹配。结果两端必须落在完整源码点、完整折叠段及启用时的完整连字展开边界；结果返回原文 UTF-16 起止范围，原有换行 / Tab / 字形仍显示。既有非重叠、最多 2,000 项与安全 text node / mark.textContent 高亮合同保留。两个选项都关闭时保持原字面 indexOf 行为；只开连字时仍遵守发布 39 的独立合同。

控件只用于 PDF：页面模式、忙碌或无正文时禁用。成功打开另一文档或准备新编辑工作区时重置，取消打开保留旧文档状态。取消 / 失败打开保留既有选项和匹配；非 PDF 不启用兼容选项。

## 真原件问题与限制

未改造 Mozilla `tracemonkey.pdf`（1,016,315 bytes）的标题查询 `Type Specialization for Dynamic Languages` 在既有字面与仅连字两种模式均不命中。启用空白兼容后，无论连字选项开关，均恢复原文范围：

- 新执行的发布 40 冻结 stable / MSRV CLI：`[27,68)`
- 新执行的实际 PDF.js 5.6.205、pypdf 6.10.0、Poppler：`[25,66)`

四种提取来源的对应原文切片均为 `Type Specialization for Dynamic\nLanguages`（其中 `\n` 为原有换行），高亮保留换行。Rust 两个 CLI 的完整正文与历史正文相同；原件及受检代码前后身份不变。pypdf / Poppler 只作独立诊断，不是应用的高亮坐标。此处使用既有冻结 CLI 重新提取，不是重新构建或重跑 Cargo 全套。

提取器并不能始终区分软换行、段落或栏边界。即使只跨一个换行，也可能连接视觉上不同的段落或栏；因此选项默认关闭，屏障规则只是已提取字符串上的有界保护，不保证理解页面结构。不同提取器的原文范围和切片可不同，不改写正文来强求相等。

此项不修复 Rust 字体 / 编码失败、缺字、PDF 页面渲染或复杂阅读顺序；不扩大既有页数 / 正文长度限额。浏览器、原生交互、系统剪贴板及实际页面渲染不能由纯 helper 或 DOM 运输替身检查代替。

## 验证与复现

- 826 个不同前端检查 = 原 807 + 新 19，TypeScript / 隔离暂存工作区 `npm run build` 通过；保留既有大 chunk 提示，不据此声明启动性能达标
- 独立扫描器 / 穷举完整 token oracle：9,520 个比较通过；旧字面 / 仅连字默认行为另有 2,000 个比较通过，验证结果上限 2,000
- 实际 `renderText` AST 提取与最小 DOM 运输：原文完整重建、安全 HTML-like 文本和非 PDF 门控通过；不是真实浏览器 / 原生验收
- Rust / 后端 / vendor / PDF.js 正文拼接与发布 40 `50ed3251e6433fa07bfdb88fe90c48ed5fc777de` 逐文件字节相同，235 个受检文件零差异。本轮未重跑 Cargo 或后端套件；发布 40 各 878 项 Rust、各 35 项后端仍仅为历史证据

仓库中的纯模块检查可单独复现：

```sh
npx playwright test --config tests/reading-search.config.mjs
npx playwright test --config tests/pdf-text.config.mjs
npx tsc --noEmit
npm run build
```

上述是重跑入口，不表示这些单独命令均已在本轮执行；本轮前端命令与结果以最终报告为准。独立原件验证使用 Node PDF.js 的 legacy adapter；真实 PDF 原件未随本轮提交，临时验证器依赖本机证据和冻结 CLI，不作为可移植测试包。

证据位置：

- 最终独立 helper / DOM / 四来源：`tmp/release41-independent/result-v2.json`
- 冻结 CLI 新提取与前后身份：`tmp/release41-independent/fresh-rust-result.json`；两种模式 / 两个选项的实际匹配：`tmp/release41-independent/fresh-rust-matcher.json`
- 未改动范围：`tmp/release41-doc-drafts/source-scope.json`
- 完整前端报告：`tmp/release41-frontend/attempt-eftndzgs/report.json`

最初前端 driver 把旧检查数断言误写为 826，在执行测试前停止；校正为旧 807 + 新 19 后通过。原失败日志保留为 `tmp/release41-frontend/launch.stderr`，不计为测试失败或成功。完整运行命令记录于 `tmp/release41-frontend/command.txt`；独立最终验证入口为 `tmp/release41-independent/verify-v2.mjs`。这些临时证据不是仓库分发的重跑依赖。

代码入口：`ui/reading-search.ts`（投影与原文范围）、`ui/main.ts`（PDF 选项 / 状态与渲染）、`ui/index.html`（独立控件）；`tests/ui/reading-search.spec.mjs` 和 `tests/ui/pdf-session-identity.spec.mjs` 覆盖新查找与状态合同。

历史边界另见 [PDF 连字兼容查找](PDF_LIGATURE_SEARCH.md) 和 [PDF 正文分隔](PDF_TEXT_ASSEMBLY.md)。发布 39 的“不扩展空白”仍是其历史范围，不应倒写成当时已支持本轮功能。
