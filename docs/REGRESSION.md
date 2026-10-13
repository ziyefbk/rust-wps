# 当前跨格式回归记录

## 当前：DOCX 隐藏文字导入保护（2026-10-13）

从 `f6aa8f6c50c364b753324c9cb5fa254605bd3542` 增加保守隐藏文字审计/导入拒绝和中文说明。8 项新增 Rust 测试覆盖隐藏 run、默认值/样式、模糊 toggle、前缀差异、关系定位的样式、实际主部件诱饵、Content Types 大小写/UTF-16，以及 false-only 的草稿、导出、重开和原件/已有输出保护。未增加隐藏文字编辑，也未修改 vendor、Excel 或 PPT 功能。

### 已实际执行

| 检查 | 命令/方法 | 结果 |
| --- | --- | --- |
| 旧版回归失败 | 在独立基线工作树加入新测试，仅运行 `hidden_direct_runs_are_refused_before_the_ir_can_drop_text` | 预期失败，退出 101，旧审计缺隐藏风险；`tmp/baseline-red.log` |
| 旧版实际丢失 | `python3 scripts/check-hidden-text.py --cli tmp/baseline-rust-wps --expect-baseline-loss --render` | 直接/继承隐藏 run 在旧版导入及导出后丢失，独立 python-docx 确认；`tmp/hidden-text/run-o8w_g5uz/report.json` |
| 新增核心测试 | `cargo test --locked --test word_hidden_text` | 8 通过 |
| 全部核心 | `cargo test --locked`；`cargo +1.88.0 test --locked --target-dir tmp/msrv-target` | Rust 1.99.0 / 1.88.0 各 153 通过；同一测试集，不重复累计 |
| vendor | `cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --target-dir target --quiet` | 1,255 通过；同路径 `cargo clippy --lib ... -- -D warnings` 通过 |
| 核心格式 | `cargo fmt --check`（1.99） | 通过；未重排 vendor |
| 新版独立文件 | `python3 scripts/check-hidden-text.py --cli target/debug/rust-wps --render --corpus tmp/hidden-corpus/testWORD_features.docx` | 通过；`tmp/hidden-text/run-2g_ibxa3/report.json` |
| 前端构建 | `npm run build` | TypeScript 与 Vite 通过；既有大 chunk 提示仍存在 |
| 无浏览器前端/模型 | Linux 配置下显式选择无需 page/browser fixture 的 Playwright 用例 | 39/39 通过，含真实 Rust 导出和独立 openpyxl；`tmp/ui-no-browser-linux.log` |
| Python/JS/补丁检查 | `python3 -m py_compile scripts/check-hidden-text.py`、`node --check tests/ui/word-hidden-text.spec.mjs`、`git diff --check` | 通过 |

三个原有 Apache POI DOC/DOCX 样本按清单下载并核对 SHA-256；在 Linux 用 Python 执行与 `check-corpus.ps1` 相同的 CLI 检查、两份 DOC 转存、DOCX 替换/重开和源哈希校验，再运行 `python3 scripts/check-corpus-independent.py tmp/corpus/linux-uzvehcrb` 通过。输出段落数组和 1 个手动分页一致；这不是执行 PowerShell 脚本，也不是独立 DOC 二进制解析/视觉等价证明。

独立 hidden 脚本区分三份 python-docx 定向生成包与固定 Apache Tika 上游测试文档。Tika 样本 8,169 字节，SHA-256 和出处见 `scripts/hidden-text-corpus.json`；另含批注/修订，因此明确断言隐藏风险，不能用任意拒绝代替新分支验证。python-docx 能读到两个隐藏 run，LibreOffice→PDF 不显示它们。输入和已有输出哈希保护检查通过。生成/Tika PDF 的 PNG 已查看；LibreOffice 26.8 开发版有字体缓存不可写提示，但返回 0，文字断言通过。字体、复杂排版及 Word/WPS 一致性不在本项证明范围。

### 失败、受阻与未执行

- 完整严格核心 Clippy **未通过**：Rust 1.99 在既有 `src/spreadsheet_xls.rs:129` 报 `chunks_exact_to_as_chunks`，基线同命令复现相同失败；Rust 1.88 在既有 `src/presentation_import.rs:97` 报 `uninlined_format_args`。未为本项改其他格式。分别用 `-D warnings -A clippy::chunks_exact_to_as_chunks` / `-D warnings -A clippy::uninlined_format_args` 的命令行限定排除后通过；这是带明确排除的检查，不是原始严格检查通过。
- 浏览器套件完整发现 203 项/57 个文件。新增隐藏文字 UI 尝试启动两次（含执行权限升级）都在 Chromium 154 创建 socket 时因 `Operation not permitted` 退出，测试体未执行。39 项无浏览器检查通过，其余 **164 项浏览器相关检查未验证**，不宣称全量 UI 通过。
- Linux 配置仅在忽略的 `tmp/playwright.linux.config.mjs` 中改用系统 Chromium；临时 CLI `.exe`/Python 路径适配用于原有 Windows 硬编码测试。没有把这些适配提交为产品修改，也没有据此声称 Edge/WebView2 覆盖。日志见 `tmp/hidden-text-ui-linux.log`、`tmp/ui-list-linux.log`。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` 实际尝试，因 `glib-2.0.pc` 缺失而构建失败（退出 101）；环境也没有 GTK/WebKit 开发包。桌面后端测试体、桌面 Clippy、打包和 Windows 原生回归均未执行完成，不能使用下方历史通过结果替代。
- 首次 `npm ci` 因默认 home 缓存不可写失败；改用工作区 `--cache` 后成功安装 72 包，锁文件未改。Rust 工具链也仅安装到工作区。没有安装额外系统库来假装 Windows 原生验收。

完整边界、复现命令和后续必补检查见 [HIDDEN_TEXT.md](HIDDEN_TEXT.md)。目标继续进行。

## 历史：Word 显式字体名与主题字体导入

字体名称作为 `fontFamily{name}` 文字标记保存在草稿与自有 DOCX 模型中；写出的每个显式 run 只有一个 `w:rFonts`，其中 ascii、hAnsi、eastAsia、cs 均为所选字面名称。外部 DOCX 按实际文字脚本拆分并保留不同字体片段，解析直接格式、字符/段落/标题样式及有限主题语言；不能解析的受引用主题字体由 `DOCX_FONT_THEME_UNRESOLVED` 提示。旧 V1 未设置显式字体的编码部件仍按原样输出。

本轮 `cargo test --locked --quiet` 全部通过，新增 9 项字体集成用例覆盖有效/无效名称、草稿和模型篡改、代码文字、四个 OOXML 字体槽、主题语言/缺项警告、真实标题样式及结构上限。`cargo clippy --locked --all-targets -- -D warnings`、前端 `npm run build`、普通桌面后端 10 项测试、vendor 1,255 项测试及严格 Clippy 均通过。vendor 原始 128 文件哈希核对后有 9 文件差异、119 文件一致；两份补丁反向检查通过。

`npx playwright test` 运行 202 项，199 项通过、3 项旧断言失败，耗时 5.7 分钟。失败的共同原因是外部重导入现带有实际字体名，而旧断言仍按无字体名比较；新功能、其他 199 项行为均通过。更新三处断言以明确期望解析出的 Arial / Microsoft YaHei 后，相关三个文件 `npx playwright test tests/ui/word-replace-all.spec.mjs tests/ui/word-scripts.spec.mjs tests/ui/word-strike.spec.mjs --reporter=line` 共 9 项通过。新增 6 项字体 UI 用例覆盖 CSS 字体名粘贴、局部/混合/光标/代码文字、保存锁、撤销、结构上限，以及真实 CLI 导出和独立 python-docx 读取。上述两次运行共同覆盖 202 项，未声称修改断言后又执行一次全量。

三份固定 DOC/DOCX 语料的 `scripts/check-corpus.ps1` 和独立 python-docx 核对通过，目录 `tmp/corpus/7ed5c405-f2be-4cfe-b933-16f499428a53/`；配对非空正文、段落结构及 1 个手动分页一致，源件不变。该核对不包含 DOC 独立二进制解析或视觉排版比较。

真实 WebView2：`npx tauri build --no-bundle --debug --features native-smoke` 后运行 `node scripts/native-word-font-family-smoke.mjs` 通过，证据 `tmp/native-word-font-family/run-PsZcVu/`。两段实际键盘输入分别为普通/代码文字，均设置 Courier New、13.5 pt、RGB `1A6B3C`；撤销/重做、V2 磁盘草稿与 worker 重开保留标记。独立 python-docx 在两段 run 中各读到单一 `rFonts` 的四槽 Courier New 和 `szCs=27`；原生 PDF 的 BaseFont 为已安装的 CourierNewPSMT 子集，pypdf 实测两段约 13.494 pt 和目标 RGB（1/255 容差内）。打印未改草稿，源 DOCX 哈希不变；编辑器截图已检查。此证据仅覆盖两段水平 Latin 字形，不覆盖缺字替代、CJK 字形、完整塑形和 Word/WPS 分页等价。

已补充 [dot 接手说明](DOT_HANDOFF.md)。目标继续进行，字体替代/主题动态关联和 DOC/PDF、Excel、PPT 路线图缺口均未关闭。

## 历史：Word 版本化默认样式

新建文档及外部 DOC/DOCX 编辑副本使用 `standard-v2`。正文声明 Arial / Microsoft YaHei、11 pt、黑色和 1.5 倍行距；三级标题为 18/15/13 pt、粗体，段后 6 pt、标题段前 12 pt，A4 四边 25.4 mm。旧模型省略 profile，继续使用原 V1 编码与显示规则；旧缺省样式的历史差异没有通过静默迁移消除。

根节点 `doc.attrs.styleProfile` 随编辑、撤销、草稿、DOCX 和打印克隆保留。草稿仍使用原文件名，内部版本为 1 或 2；嵌入模型部件名、版本和 profile 必须一致。未知/null profile、双模型、删部件后伪造哈希及额外包部件明确拒绝。旧 fixture 再编码逐部件字节相同。

本轮核心 `cargo test --locked --quiet` 共 136 项、普通桌面后端共 10 项通过。核心与桌面严格 Clippy 全 targets（桌面含 all features）、核心 fmt 通过。8 项新增核心用例验证版本协议、默认样式、常规/复杂文字属性、草稿及包一致性；5 项新增 UI 用例验证真实 CLI/独立 python-docx 输出、重开、草稿队列、打印快照与实际 PDF 字号/边距、非法版本不替换当前文档和外部导入。

`npx playwright test` 全量运行 196 项，194 项通过，2 项失败，耗时 6.2 分钟。失败均是上下标/删除线旧用例对外部重导入模型的精确期望没有包含 V2 已声明的黑色；实际差异只有每个文字节点新增 `textColor=000000`。自有包直接重开仍保留“无显式颜色”的原模型，外部重新导入则解析已声明的默认颜色，两条路径有意不同。此次全量包含最后的 code 字体 Arial 回退补充，前端资源在测试期间没有重建。

核对失败产物后，仅更新这两处精确期望，并新增独立 python-docx 默认黑色断言；原上下标/删除线与草稿无显式颜色断言保留，产品代码未因此修改。随后 `npx playwright test tests/ui/word-scripts.spec.mjs tests/ui/word-strike.spec.mjs tests/ui/word-style-profile.spec.mjs` 共 10 项通过，25.4 秒，覆盖两项原失败及全部 5 项 V2 专项。其余 186 项沿用本轮全量的成功结果，没有把定向复测描述为又一次全量运行。

固定真实 DOC/DOCX 语料及独立 `scripts/check-corpus-independent.py` 通过，目录 `tmp/corpus/d1aac369-abed-43a9-9644-32013ea2e7a2/`。独立 python-docx 核对配对段落数组、替换正文与 1 个手动分页，源文件不变。当前轮没有修改 vendor，1,255 项解析库结果沿用下方历史记录，未重复计为本轮运行。

原生检查先以 `npx tauri build --no-bundle --debug --features native-smoke` 构建：

- `node scripts/native-word-style-smoke.mjs` 通过，目录 `tmp/native-word-style/run-3w1Rep/`。真实键盘/标题菜单输入、V2 磁盘草稿、CLI DOCX 导出及 worker 重开通过；独立 python-docx 检查默认字体、sz/szCs、三级标题和四边 25.4 mm。原生 PDF 为 1 页 A4，pypdf 实测正文/标题约 10.990/17.991/14.993/12.991 pt，黑色 Arial / Arial Bold，另有 Microsoft YaHei 中文字体资源，正文横坐标约 72 pt。保留原始 PDF 矩阵和字体证据，草稿及源 DOCX 哈希不变，编辑器截图已检查。首次脚本错误地假设末尾标题后没有空段；核对 Tiptap 默认 TrailingNode 后，改为严格允许最多一个末尾空段，四个非空探针及物理属性检查不变。
- `node scripts/native-word-font-size-smoke.mjs` 通过，目录 `tmp/native-word-size/run-mngOV5/`。显式 13.5 pt / `szCs=27` / RGB `1A6B3C` 经草稿和独立 DOCX 读取保留；PDF 实测约 13.4936 pt、填充 `[0.102,0.4196,0.2353]`，撤销/重做及打印不改草稿通过。
- `node scripts/native-doc-pages-smoke.mjs` 通过，目录 `tmp/native-doc-pages/run-rw3uX6/`。真实 DOC worker/导入保留 1 个手动分页，PDF 为两页 A4，各页正文标记分离，原件及隔离输入哈希不变。

补齐 code 的 Arial 字体回退链后，重新构建 native-smoke 并再次运行默认样式探针通过，最终证据目录为 `tmp/native-word-style/run-qR7But/`；字号、黑色、Arial、72 pt 横坐标、V2 worker 重开和源文件不变均继续通过。上述显式字号/颜色与 DOC 分页两项使用补充前的嵌入前端执行；差异仅为 CSS code 回退链，不涉及其受测文字。

这些检查证明受测默认样式声明、水平 Latin 字体/字号、黑色和左边距的一致性；DOCX 通过 CLI 导出实际磁盘草稿，不覆盖系统另存对话框。浏览器与 Word 使用不同排版引擎，字体回退、行高计算、中文/复杂文字塑形、自动分页及复杂版式尚不保证相同。

原生验证结束后，`npx tauri build --no-bundle --debug` 成功，已恢复不含 native-smoke 的普通桌面程序。前端保留既有的大 chunk 构建提示，本轮不据此声称启动性能或完整 Office 兼容达标；Goal 继续进行。

## 历史：Word RGB 颜色、主题解析与包完整性

RGB 颜色贯通选区/后续输入、色盘/HEX/黑色色块、草稿、撤销、清除和 DOCX 导入导出。显式颜色优先于链接默认蓝色；表格、代码和原有字号共存，拆分超预算时在修改前拒绝。主题色按 Word HSL 规则近似转为固定 RGB，标题颜色继承、缺主题时的最终备用色及严格颜色词法有回归；自动/未解析颜色明确提示，自定义主题映射与受引用的复杂主题颜色方案拒绝编辑导入。

本轮实际执行：

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 跨格式界面全量 | `npx playwright test` | 191 项通过，5.3 分钟 |
| 包完整性修复后的文字界面复测 | `npx playwright test tests/ui/authoring.spec.mjs tests/ui/word-font-size.spec.mjs tests/ui/word-text-color.spec.mjs` | 18 项通过，43.0 秒 |
| 最终 Rust 核心 | `cargo test --locked --quiet` | 128 项通过 |
| 本地解析库 | `cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --quiet` | 1,255 项通过 |
| 最终普通桌面后端 | `cargo test --manifest-path src-tauri/Cargo.toml --locked --quiet` | 9 项通过 |
| 静态检查 | 核心/桌面严格 Clippy 全 targets，解析库严格 Clippy lib；核心 fmt | 全部通过 |

191 项全量执行后，审查发现旧自有包加载器只逐项检查实际存在的部件，没有反向检查完整集合。新增用例先复现仅删 styles、仅删图片媒体、同时删两者并同步删哈希记录的三种误接受，再补齐 expected/actual 完整部件键集合相等。修复后上述 18 项界面和完整核心/桌面检查通过，编码字节规则未改；历史无字号包的部件字节兼容测试仍通过。解析库 1,255 项执行于此末尾根加载器修复之前，此后 vendor 未改。前端资源在每次界面测试期间保持固定。

新增 9 项颜色 Rust 集成测试覆盖 RGB/非法值/草稿与包篡改、链接优先级、标题完整颜色继承、24 组主题与备用色的有/无主题分支、12 种非默认映射拒绝、默认映射放行及 clrScheme/fmtScheme 变换区别。HSL 预期由独立 Python colorsys 固定计算；微软 shade 示例的一个色阶量化差异明确记录于 EXPLORATION.md，不宣称 Word 像素一致。5 项颜色 UI 测试包含真实 CLI 导出与独立 python-docx 逐 run 读取、色盘/黑色色块、局部/混合/光标、保存锁、粘贴及结构预算。

三份固定 DOC/DOCX 真实语料经 `scripts/check-corpus.ps1` 与独立 `check-corpus-independent.py` 通过，目录 `tmp/corpus/cc0ce5bb-4061-45ef-9358-c679728c075d/`：非空正文、完整段落数组及 1 个显式分页匹配；原件保持不变。此语料回归早于末尾加载器集合检查，后续定向文字回归覆盖正常自有包重开。vendor 原始 128 文件哈希匹配，本地仅记录的 8 文件变化，其余 120 文件不变，两份补丁反向检查通过。

最终原生 WebView2 检查在包完整性修复后执行，先以 `npx tauri build --no-bundle --debug --features native-smoke` 构建：

- `node scripts/native-word-font-size-smoke.mjs` 通过，证据 `tmp/native-word-size/run-zLsBb3/report.json`。实际 IPC 草稿记录半点 27、RGB `1A6B3C`；颜色撤销/重做保留字号和文字。从实际草稿经 Rust CLI 生成 DOCX，独立 python-docx 读取 13.5 pt、`szCs=27`、RGB `1A6B3C`，每 run 只有一个颜色节点。原生 PDF 共 1 页，pypdf 在水平 Latin 探针测得约 13.4936 pt，填充 RGB 为 `[0.102, 0.4196, 0.2353]`，与目标 `[26,107,60]/255` 在 1/255 容差内；打印前后草稿字节不变。原生编辑窗口截图已检查。
- `node scripts/native-doc-pages-smoke.mjs` 通过，证据 `tmp/native-doc-pages/run-gaEH4k/report.json`。真实 DOC worker、编辑器和磁盘草稿均保留 1 个分页，独立 pypdf 核对两页 A4 及每页正文标记，原件及隔离输入哈希保持不变。

原生 DOCX 验证使用 CLI 导出实际草稿，不覆盖系统另存对话框；PDF 探针只证明受测水平文字的物理字号和纯 RGB 填充，不证明完整颜色管理、所有脚本塑形、字体替代或 Word 版式一致。默认字体/字号/颜色的编辑、打印和 DOCX 差异仍待版本化统一，链接下划线颜色仍可能不同。

原生验证结束后，`npx tauri build --no-bundle --debug` 成功，恢复普通桌面可执行文件。前端仍有既有较大 chunk 提示，未把构建通过当作加载性能验收；Goal 继续进行。

## 历史：Word 半点字号、默认样式与标题字号

本轮字号贯通 UI → 草稿 → Rust DOCX → 独立读取，支持 1–1638 pt、0.5 pt 精度。修复全选后立即局部选区的同步、输入转焦提交、代码格式共存和修改前保存预算；粘贴不把字号留在硬换行等非文字节点上。解析库补齐默认段落样式、标题有效字号，以及常规/复杂文字字号单位的精确换算。上游来源、七文件修改和两份独立补丁见 [vendor/README.md](../vendor/README.md)。

最终源码的实际检查：

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 跨格式全部界面 | `npx playwright test` | 186 项通过，5.8 分钟 |
| Rust 核心 | `cargo test --locked --quiet` | 118 项通过 |
| 本地解析库 | `cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --quiet` | 1,248 项通过 |
| 普通桌面后端 | `cargo test --manifest-path src-tauri/Cargo.toml --locked --quiet` | 9 项通过 |
| 静态检查 | 核心/桌面 `cargo clippy --all-targets -- -D warnings`，解析库 `--lib` | 全部通过；核心 `cargo fmt --check` 通过 |

字号的 7 项 UI 用例包括局部/混合选区、半点、后续输入、清除与撤销、锁定/非法值、粘贴软换行、上下标/代码/表格、9,999 节点的拆分拒绝；真实 Rust 输出经 python-docx 检查各 run 的 pt 和 `szCs`。7 项 Rust 专项包含半点边界、JSON 属性拒绝、草稿/包篡改、默认样式/标题/复杂文字继承、带单位字号及旧版自有 DOCX 包部件字节兼容。界面全量运行期间未重建前端资源。

固定三份真实语料复测与独立 python-docx 检查通过，成功目录 `tmp/corpus/37c0961f-441f-41e1-8c16-5164a63d3e7f/`。DOC 转存段落数组和 1 个显式分页与配对 DOCX 一致，替换结果也完全匹配。首次以 Windows PowerShell 5 执行时，UTF-8 无 BOM 的脚本把中文替换参数误读，独立检查成功捕获；脚本改为 UTF-8 BOM 后重跑通过，失败目录 `03038790-3227-4570-b1fe-a2f28502fe7e/` 不计成功。

首次字号选区测试及随后标题字号集成测试曾暴露实际问题，修复后才执行以上全量结果。支持字号不等于完整 Word 排版：缺省编辑/打印与 DOCX 样式尚未统一，字体名称/颜色、分节布局、完整样式、真实输入法及系统剪贴板仍待实现或验收。

原生 WebView2 验证：

- `node scripts/native-word-font-size-smoke.mjs` 通过，证据 `tmp/native-word-size/run-wWdSvj/report.json`。真实 Tauri IPC 写入草稿半点值 27，撤销/重做保留文字；从该草稿经 Rust CLI 导出，python-docx 读取字号 13.5 pt、`szCs=27`。真实 WebView2 PDF 的水平 Latin 探针经 pypdf 文本/页面矩阵计算为约 13.4936 pt（允许 0.15 pt 误差），共 1 页；打印前后草稿字节不变。这条 DOCX 路径使用 CLI，未覆盖系统另存对话框。
- `node scripts/native-doc-pages-smoke.mjs` 通过，证据 `tmp/native-doc-pages/run-jW7Oo9/report.json`。真实 DOC worker/导入、磁盘草稿均保留 1 个分页；PDF 为两页 A4，页一/页二标记分开，原件及隔离输入哈希不变。

原生脚本先使用 `npx tauri build --no-bundle --debug --features native-smoke` 构建。字号探针证明受测文字的物理字号，不证明所有字体替代、复杂文字塑形或完整 Word 视觉版式。

验证后 `npx tauri build --no-bundle --debug` 成功，已恢复不含测试 feature 的普通桌面可执行文件。前端仍有既有较大 chunk 提示，未据此宣称启动/加载性能达标。Goal 保持进行中。

## 历史：DOC 正文手动分页恢复

固定 `office_oxide 0.1.13` 的本地源码补丁保留分页控制信息，区分手动分页和分节终止，不从空行推断。来源、128 个上游文件的原始哈希及四文件差异见 [vendor/README.md](../vendor/README.md)。同时修复分页旁真实空段落丢失，并对只恢复部分结构化正文的文件拒绝编辑转换。

本轮实际结果：

- `cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --quiet`：最终 1,235 项通过。
- `cargo test --locked`：最终 111 项通过，包含两个自制二进制 DOC 的完整导入回归；原文件字节不变，正常样本转存 DOCX 重读保持空段与分页，不完整样本明确拒绝。
- 普通桌面 `cargo test --manifest-path src-tauri/Cargo.toml --locked`：9 项通过；核心与桌面 `cargo clippy --all-targets -- -D warnings`（桌面指定 manifest）均通过。
- `npx playwright test`：179 项通过，5.4 分钟。此全量运行早于末尾正文完整性保护和空段保留两项补充；补充后定向重跑 DOC/DOCX 导入与打印，共 9 项通过（25.9 秒）。
- 固定真实语料 `scripts/check-corpus.ps1` 通过，输出目录 `tmp/corpus/dcbaf5de-5069-4b86-a2c6-c3575608c69f/`。独立 `check-corpus-independent.py` 验证配对 DOC 转存结果与参考 DOCX 的段落文字数组一致，且均有 1 个显式分页；DOCX 替换检查继续通过。
- 真实 WebView2 `node scripts/native-doc-pages-smoke.mjs` 通过，证据 `tmp/native-doc-pages/run-2iEb9t/report.json`。编辑器与磁盘草稿均有 1 个分页，pypdf 独立读取两页 A4，第一页含 `This is page 1`、第二页含 `This is page two`，互不串页；原件与隔离输入哈希不变。首次脚本运行在打开后的等待步骤超时，未计成功；补充 DOMContentLoaded 等待与失败诊断后重跑通过。

复现自制输入：`python scripts/make-doc-pagination-fixtures.py --check`。真实语料独立检查：`python scripts/check-corpus-independent.py tmp/corpus/dcbaf5de-5069-4b86-a2c6-c3575608c69f`。原生验证需要先用 `npx tauri build --no-bundle --debug --features native-smoke` 构建，结束后恢复普通构建。

新原生脚本再次运行也通过，目录 `tmp/native-doc-pages/run-Ot54zS/`。以上证明受支持正文的显式分页恢复及样本的逐页文字，不证明完整 Word 视觉版式。分节、自动分页、复杂表格/列表内分页、更多中文 DOC 与真实输入法仍待实现或验收；Goal 继续进行。

验证结束后，`npx tauri build --no-bundle --debug` 成功，已恢复不含 native-smoke 的普通桌面构建。前端构建仍有既有的较大 JS chunk 提示，未当作加载性能达标的证据。

## 历史：查找与替换

全部替换加入后：19 项 Word 相关回归通过（49.5 秒）；补充草稿/节点预算用例后，4 项批量替换用例复测通过（17.4 秒）。真实 WebView2 批量替换正文和表格、单步撤销、磁盘草稿、Rust DOCX 导出及独立 python-docx 读取通过，产物 `tmp/native-pdf/run-1pvUV9/`；已有单处替换、A4 PDF、拒绝覆盖及损坏输入保护继续通过。未重跑下面 173 项全量基线，未据此扩展 DOC 版式兼容声明。

当前匹配替换加入后：16 项 Word 相关界面测试通过，覆盖替换/删除、首字符样式和链接、撤销分组、草稿、无效输入与已有编辑/退出流程；真实 WebView2 替换、磁盘草稿、Rust 导出和独立 python-docx 验证通过，产物 `tmp/native-pdf/run-sTsvMt/`。本轮未重跑下面的 173 项历史全量基线。

补齐 U+FFFE/U+FFFF 的修改前拒绝后，4 项查找/替换用例复测通过（24.4 秒），包含无效输入不改正文、草稿及历史。原生证据早于这项校验补充。

## 历史：Word 正文查找加入后的完整回归

随后增加了 DOC 分页丢失的明确导入提示：9 项 Rust 导入测试、3 项导入界面测试、核心严格 Clippy 与前端构建通过。下面 173 项记录早于这一提示修改，不作为修改后重新执行的全量结果。

实际执行 `npx playwright test`：173 项通过，4.7 分钟，退出码 0。包含删除线、上/下标、清除字符格式、结构按键选区同步及正文查找；同时覆盖原有 PDF、Excel、PPT 流程。运行期间未重建前端资源。

`cargo test` 全部通过；`cargo test --manifest-path src-tauri/Cargo.toml` 9 项通过；两端 `cargo clippy --all-targets -- -D warnings`（桌面端加对应 manifest 参数）均通过。以上均来自本轮实际命令退出码 0，不复用历史结果。界面仍主要使用模拟 Tauri 桥，不能替代原生输入法或系统对话框验收。

`scripts/check-corpus.ps1` 三份固定样本复验通过，输出目录 `tmp/corpus/66f3a4bb-f051-4ea9-8303-17319d96a517/`。新增独立 python-docx 检查可复现：

```text
python scripts/check-corpus-independent.py tmp/corpus/66f3a4bb-f051-4ea9-8303-17319d96a517
```

独立读取证明配对 DOC 转换的非空段落正文一致，DOCX 替换后的全部段落文字及显式分页一致；同时发现 DOC 转换结构不一致：参考 DOCX 有 1 个显式分页，DOC 转换输出为 0，边界出现空段落。该脚本明确报告差异，不将非空正文一致解释为版式兼容；也不提供独立 DOC 二进制解析或渲染验证。

本轮没有重跑原生冒烟；最近 Word 原生查找、草稿字节不变及 A4 PDF 证据仍为 `tmp/native-pdf/run-p6AYUU/`。目标继续进行，复杂排版与原生输入、PDF 页面操作、Excel 筛选/样式、PPT 基本形状等缺口仍有效。

## 历史：164 项基线与后续定向验证

此处 164 项全量基线在新增文字删除线及上下标按钮之前执行，不作为该新增之后代码的全量证明。新增之后：18 项既有文字/打印/退出相关测试通过，新删除线用例及独立 python-docx 复测通过，原生 WebView 选区/草稿与 A4 PDF 脚本通过，详见 EXPLORATION.md。

本记录在加入 Excel 自动重算、工作簿键盘输入/查找，以及 PPT 画布选择、文本/图片拖拽、中断过滤、比例恢复、方向键微调和右下角尺寸手柄之后执行。它替代先前 140 项界面回归作为当前源码的全量记录；不表示完整 Office 兼容或整个开发目标完成。

| 检查 | 实际命令 | 本次结果 |
| --- | --- | --- |
| 全量界面/前端模块 | `npx playwright test` | 164 项通过，4.4 分钟 |
| Rust 文档核心 | `cargo test` | 全部通过 |
| 普通桌面后端 | `cargo test --manifest-path src-tauri/Cargo.toml` | 9 项通过 |
| 核心静态检查 | `cargo clippy --all-targets -- -D warnings` | 通过 |
| 普通桌面静态检查 | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | 通过 |

界面测试使用最近一次普通 Debug 桌面构建中的同版前端资源，运行期间没有重建 dist。`tmp/ui-tests/.last-run.json` 当前为 passed 且 failedTests 为空；该目录会被后续运行更新。核心结果来自实际命令退出状态和各组测试输出，不引用历史 `tmp/core-regression.log`。

覆盖文字编辑、表格、段落对齐、链接、手动分页、草稿/退出失败保护、DOC/DOCX 基础导入、PDF 本机密码/搜索/渲染/打印快照；XLS/XLSX 稀疏读取、编辑类型、日期系统、数字格式、复制粘贴、有限计算及自动调度、排序、查找、工作表操作和导入；PPT/PPTX 阅读、备注/母版、基础导入、文字替换、图片、页面/文本框复制、层次、隐藏、历史/草稿、播放、换行/对齐、尺寸切换、画布移动与微调、对象尺寸及近似 PDF。

界面按需要调用真实 Rust CLI 和独立 python-pptx/openpyxl 消费者，部分用例真实写入并重读 Office/PDF 文件；不是每个测试都使用真实文件。界面的 Tauri IPC、草稿运输层、原生关闭及文件对话框多数由测试桥模拟，页面由真实 Edge 执行。后端测试另验证 worker 协议/截止/响应限制、Windows Job 内存限制及 PDF 原子发布。

本轮未重跑原生 WebView 冒烟；最近一轮原生文本/图片尺寸、键盘、拖拽、磁盘草稿、PPTX 和六页 PDF 证据位于 `tmp/native-pdf/presentation-YZby9O/`，方法和边界见 [PRESENTATIONS.md](PRESENTATIONS.md)。Word/Excel 原生证据见各格式文档。上述证据不能替代真实中文输入法候选、系统剪贴板/对话框、复杂 Office 视觉比较、发布性能和跨平台验收。

阶段仍未完成：文字/PDF 需更多真实语料与复杂排版/原生交互证据；Excel 缺少筛选、字体边框等样式、完整依赖重算及 XLS 编辑转换；PPT 缺少基本形状、混合对象层次和复杂模板/对象兼容。当前能力矩阵与原阶段要求见 [ROADMAP.md](ROADMAP.md)。


上下标与结构按键选区同步加入后：19 项相关 Rust 测试、24 项界面回归及两个跨段合并补测通过，核心严格 Clippy 通过。没有重跑上述 164 项全量基线，不能将该历史结果当作当前修改后的全量证明。


修复后原生 Word 脚本无诊断延迟连续两次通过（上/下标互斥、撤销、磁盘草稿、快速分段和输入、A4 PDF、拒绝覆盖与失败保护）；运行产物见 EXPLORATION.md。


清除字符格式加入后，18 项相关界面回归及两个清除用例复测通过；上面的全量基线仍早于该修改。


编辑正文查找加入后：20 项相关文字/退出回归通过；补充扫描节点上限后两个查找用例复测通过。此前全量基线不包含该功能。
