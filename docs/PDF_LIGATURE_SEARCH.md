# PDF 正文连字兼容查找

发布 39（2026-10-15）：本地验证已完成；范围与限制如下。

PDF 正文查找新增可选“连字兼容匹配”，默认仍按字面匹配。仅将 ﬀ / ﬁ / ﬂ / ﬃ / ﬄ / ﬅ / ﬆ / ſ 与 ff / fi / fl / ffi / ffl / st / st / s 对应，按完整展开边界返回原文 UTF-16 范围并安全高亮原文。不改提取结果或 PDF 页面，不扩展大小写、空白、一般 Unicode 规范化或 OCR；不同提取器的部分连字短查询可能有意不同。

## 使用与匹配边界

在 PDF 正文模式勾选“连字兼容匹配”后生效；非 PDF 不启用，页面模式、忙碌或无正文时禁用。成功打开另一文档及准备新编辑工作区时重置为字面模式，取消打开不替换现有文档状态。控件只影响正文搜索，不是页面图像搜索或编辑替换。

八项映射为 U+FB00–U+FB06 和 U+017F，源文字与查询对称投影。没有 NFKC、大小写折叠、空白合并、断行连字符合并、全角转换、组合字符等价或 OCR。缺省字面模式保持原 indexOf 的 UTF-16 语义。

兼容匹配的两端必须落在完整源码点 / 展开边界：`ﬃ` 可被 `ffi` 匹配，不能仅被 `f` 或 `fi` 匹配；源码若已是三个字符 `ffi`，短查询可以命中完整的单个字符。故不同提取器先展开与否可能产生不同短查询结果，这是有意限制，不是统一全文规范化。兼容模式不把完整代理对内部当作端点；无效部分候选后逐单位继续搜索，不能跳过后续合法命中。

结果是不重叠的原文 UTF-16 起止范围，最多 2,000 项，界面沿用 2000+ 计数。原文始终不改：未命中片段用 text node，高亮用 mark.textContent，保留原字形、HTML-like 字符和查询外文本，不把文档文字解释为 HTML。

## 真原件与提取器差异

未改造 Mozilla 连字 PDF 为 32,408 bytes，SHA-256 `3a6948ca33558a0f1c9dabf70902e14e263e814b6a12da44ba8ec9439c0bafdd`。Rust、实际 PDF.js、pypdf 与 Poppler 的完整字符串分别记录；ASCII 查询 `ffifflstst` 在两个应用字面路径均无命中，兼容模式恢复其完整源范围：

- Rust：`[11,15)`，原始切片 `ﬃﬄﬅﬆ`
- PDF.js：`[12,22)`，原始切片 `ffifflſtst`

这两个原文范围和切片本来就不同，不能比较数值相同或改写显示文本来凑一致。pypdf / Poppler 的对应行偏移为 `[9,13)`，仅用于独立诊断，不是应用高亮坐标。

原始源页面的 72 / 144 dpi 整页图已检查，短连字行可读且未见裁剪。这是原件诊断图，不是新产品渲染效果；本轮没有页面渲染改进声明。该原件再分发权未审完，不随代码提交 PDF 二进制。既有纯 helper / 模拟 DOM 测试足以随仓库重跑，不另外打包私有历史验证器。

## 已通过验证及限制

- 774 个不同前端检查 = 原 752 + 新 22，TypeScript / build 通过并独立接受
- 独立 39 个案例 / 234 个实际 helper 与 renderText DOM 运输检查通过；保持旧字面行为、完整展开边界、原文范围、非重叠 / 上限和安全文本展示。不是浏览器或原生交互验收
- 六份真实 PDF 新基线共 22 命令：4 份 Rust 正文提取成功；mixedfonts / noembed-sjis 两份已知编码不支持安全失败，未当作已修复。实际 PDF.js 六份完成；pypdf / Poppler 全输出及警告另行保留，不声明所有提取器一致
- Rust / 后端 / vendor 与发布 38 精确不变；本轮未运行 Cargo、后端套件或真实浏览器 / 原生测试。各 856 项 Rust 与各 35 项提取后端只是继承的发布 38 证据，不计作发布 39 重跑

既有 1,000 页 / 2 Mi UTF-16 提取限额、取消 / 过期会话与 PDF 源身份保护不改变。连字投影只消费已取得的正文，不修复两份失败文件的 Rust 编码解码、阅读顺序、字体映射或缺字。

基线准备时曾把并发编辑后的 main.ts 误标为 release38 快照；该未使用快照及修正记录保留，未用于 22 个基线命令。实际旧 DOM 对照来自明确提交 `9c3df0d795922baee753324110f36db3198df23b` 的 main.ts，SHA-256 `69444ff44800c053a93ce378104a82e4e6421cad07a9d5922b4ae43dada4daa6`，已独立核对；基线 PDF helper 同样与该提交相同。不能把误标快照描述为已接受旧行为。

## 证据

- 前端：`tmp/release39-frontend/attempt-xkkkf_i_/report.json`，SHA-256 `1a3933d33b0d3a702ee272ce6bd9fe0bd46395848ddd799ebbe5280688b75068`
- 六源基线：`tmp/release39-independent/baseline-xxokd4f1/report.json`，SHA-256 `429eabb18eed0358a648106559521173fa0cc0c1905d9e64c4799c5db616eb8c`
- 独立实际 helper / DOM：`tmp/release39-independent/candidate-zn_kcqsh/report.json`，SHA-256 `ef3afd74bdd3cc7a3f8a09169c0b7370e0eb9abb3b51dba6a9b825dcbe6032bd`；子报告 SHA-256 `3eb47bd687e2e16e59e6e8df552494341d23a24193496292a60aac54d3cc0568`
- 快照修正：`tmp/release39-independent/SNAPSHOT-CORRECTION-v1.json`，SHA-256 `7552dc9aafa3691bed434e1c2ef9de254ee94fe0890683647c9227ddf0f488f0`

以上独立接受的是有界查找与原文展示，不是浏览器、原生键盘 / 系统剪贴板、OCR 或完整 PDF 兼容性。更早的 PDF.js 正文分隔修复独立保留，见 [PDF_TEXT_ASSEMBLY.md](PDF_TEXT_ASSEMBLY.md)。
