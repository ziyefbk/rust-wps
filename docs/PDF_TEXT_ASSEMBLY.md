# PDF.js 正文分隔与短语搜索

本轮只修复应用逐页拼接 PDF.js TextItem 时人为插入空格的问题。默认 Rust 提取、PDF.js 字体映射、页面渲染、正文查找算法和源 PDF 均未改变。

## 实际缺陷与有限修复

此前每个 TextItem 后无条件追加空格，若 PDF.js 本身返回一个空格 item，两侧又各追加一个空格，原文中的单空格会变成三个；无空格的连续字体片段也被拆开。点击“提取 PDF 正文”会用这段文字替换已有 Rust 提取，导致原先能找到的字面短语消失。

现在按原 item 顺序拼接：

- 保留 item 内及片段边界的显式空白，不在它们前后再补空格
- 保留 `hasEOL`，包括字符串长度为零的换行标记；真正空的非换行 item 不打断相邻判断
- 只有相邻无空白的片段都明确为非竖排、水平无旋转 / 无斜切的 LTR，并且宽高、六个变换数和正比例有效时，才检查连续性
- 同一基线且后一片段起点等于前一片段终点时可以直连；仅容许不超过 **0.01 PDF 用户单位及两片段比例的 0.1%** 的小数舍入差。字体名变化本身不是分词依据
- 正向间隙、倒退 / 重叠、基线跳变、旋转 / 斜切、RTL、竖排以及缺失 / 非有限几何都保守加一个空格。不会重排内容、猜测分栏 / 表格阅读顺序或把几何跳变都改成换行

继承原来的逐页 `trimEnd()` 和页面双换行分隔：页末空白仍会裁掉，因此不声称逐字符保留所有页边空白。取消、成功页保留、逐页失败记录及 cleanup 继续保持。上限仍是 **1000 页、2 × 1024 × 1024 个 JavaScript UTF-16 代码单元**，不是 UTF-8 字节上限；原有页分隔预留策略没有扩大。本轮没有另加 Unicode 规范化、忽略大小写、连字改写、断词连行或 OCR。

## 未改造来源与独立读取

复用 [固定语料清单](../scripts/pdf-snapshot-corpus.json)：Mozilla PDF.js revision `288892aadaa27f03ed0cd4724edf678c6bda8e61` 的六份未改造原件。`tracemonkey.pdf` 是实际发表的 14 页论文，SHA-256 为 `3662ff519e485810520552bf301d8c3b2b917fd2f83303f4965d7abed367e113`；其余五份是上游测试 fixture，不称为用户生产文档。大小、SHA-256、Git blob 在处理前后均核对；二进制不纳入仓库，也没有重新保存或规范化原件。

四条短语在原 helper 中各为 **0 次**，修复后与 Rust、Poppler、pypdf 的原始提取均各为 **1 次**，比较不折叠空白、不改大小写：

- `Keywords JavaScript`
- `compiling a trace speculates`
- `records a trace tree`
- `contains all the guards`

同一原件也提供不能盲目直连的反例：第 6 页 `Trace 2` → `Trace 1` 没有 `hasEOL`，但 x 终点至下一起点倒退约 **47.21**；第 2 页 `snippet.` → `Interpret` 的基线跳变约 **45.94**。修复后两组仍有分隔，没有产生粘连假词。

`copy_paste_ligatures.pdf` 是三个严格连续的字体片段。新 helper 不再插入两个空格，结果为 `abcdeffffiflffifflſtstghijklmno`；Poppler / pypdf 原始连字为 `abcdefﬀﬁﬂﬃﬄﬅﬆghijklmno`。这是分隔语义对照，**不是字形映射相等或全局 NFKC 支持**。六份语料另核对 PDF.js 非空白字符流不变，不能把这个诊断比较写成产品搜索规范化。

两份原来 Rust 不支持的编码仍退出 101 并保留明确诊断：`mixedfonts.pdf` 的 `UniJIS-UTF16-H`、`noembed-sjis.pdf` 的 `90ms-RKSJ-H`。本轮六份 PDF.js helper 提取完成不意味着这两个 Rust 缺口已修复，也不意味着中文字体或完整 PDF 兼容。

## 复现与验收层次

```text
node scripts/check-pdf-phrase-search.mjs --output tmp/pdf-phrase-validation-new
npx playwright test --config tests/pdf-text.config.mjs
npm run build
```

独立 checker 要求已有未改造来源、固定快照 CLI、PDF.js、pypdf 和 Poppler，输出目录必须全新；可传 `--corpus-dir` 与 `--rust-cli`，但 CLI 身份仍须匹配冻结快照。旧 helper 来自已发布 `c4d792784930fb004ee869752b2501bf4957ba63`，实际新 helper 从 `ui/pdf-text.ts` 编译执行，不复制一个替代算法作为产品。

checker 使用已安装 PDF.js **5.6.205 的 Node legacy adapter**。现代 Node adapter 已知缺少 `Uint8Array.toHex` 的旧失败没有重试，也没有给产品添加 polyfill。六份原件重新运行实际 helper、pypdf **6.10.0**、Poppler 和固定 Rust CLI；输出正文、TextItems / styles、每条短语计数和错误日志分别保存。

实际 `ui/main.ts` 的 `recoverPdfText` / `renderText` 通过 TypeScript AST 取出原函数，接入真实 PDF.js 文档和新 helper、最小 DOM 运输替身，核对初始 Rust 正文及连续两次重提取。每次四条短语均恰好一个 mark，小写不匹配仍为零；第二次不会重新增加空格。这不是实际 DOM、鼠标点击、浏览器渲染或 WebView2 IPC 验收。

18 项无浏览器 helper 检查覆盖显式空白、连字片段、大小写 / 组合字符 / 断词不规范化、正反向与基线跳变、旋转 / 斜切 / RTL / 竖排、所有变换项及尺寸的非法值、空 EOL、标记与空字符串、页边界、页获取 / 内容失败、取消和 cleanup，以及字符 / 分隔 / EOL / 页数上限。14 项为本轮新增。

完整命令、最终报告和历史检查复用范围见 [REGRESSION.md](REGRESSION.md)。未重试已确认受阻的 Chromium EPERM、云 localhost、Linux glib / WebKit 或离线 Windows 路线。真实点击重提取、页面渲染、原生窗口和更广的阅读顺序仍需在可用环境验收；后续 PDF.js 重新打开路径的会话身份缺口仍见 [源快照说明](PDF_SOURCE_SNAPSHOT.md)。
