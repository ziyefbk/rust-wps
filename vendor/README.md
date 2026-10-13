# office_oxide 本地补丁

`office_oxide/` 固定自 crates.io 的 `office_oxide 0.1.13` 包，包内
`.cargo_vcs_info.json` 记录提交 `7fce6094b46c6a7afd133fedbffe305ddb1929a1`。
根 Cargo.toml 通过精确版本和 path 引用；桌面端使用同一 Rust 核心。
原包的 LICENSE-MIT、LICENSE-APACHE、NOTICE、TRADEMARKS 等文件均保留。
复制时排除了 Cargo 缓存标记 `.cargo-ok` 和 Cargo.lock；本地构建产物不纳入版本控制。

`office_oxide-upstream-files.json` 保存修改前 128 个文件的 SHA-256。原 registry 包的 128 个文件均已核验匹配；本地除下列 9 个文件外，其余 119 个与原始哈希完全一致。
`office_oxide-doc-page-break.patch` 和 `office_oxide-docx-typography.patch` 是相对此基线的两组独立差异，可用于复核或迁移到新版本。
DOC 分页补丁包含前四个文件，DOCX 字号/默认样式/颜色/字体/段落间距/缩进补丁包含后五个文件。可运行 `python scripts/refresh-vendor-typography-patch.py` 从已核对哈希的 registry 原包重建后一个补丁。补丁按文本差异生成，统一使用 LF 表示行尾；源码文件的原有行尾不因此重写。

- `src/doc/piece_table.rs`：结构化段落保留正文 0x0C；纯文本提取仍映射为换行，保留链接和字符属性的 UTF-8 字节坐标。
- `src/doc/document.rs`：严格读取节边界；最终 CP 哨兵不作为新节分隔；存在分页控制字符但边界或结构不确定时给出警告；结构化段落未覆盖正文尾部时明确警告并阻止编辑转换；公开结构化段落是否可用。
- `src/doc/papx.rs`：补充 UTF-16 CP、手动分页和节结束符区分测试。
- `src/convert_doc.rs`：正文段落中的手动分页产生 IR PageBreak，保留分页两侧文字、样式和链接；保留真实 CR 结束的空/空白普通段落；表格/列表内部暂降级为只读换行并给出警告，应用据此拒绝编辑转换。
- `src/docx/styles.rs`：按 `w:default` 选择最后一个启用的默认段落样式，不猜测 Normal 名称；应用到 run/段落/隐藏属性继承。重复 ID、非法类型/布尔值、默认样式缺少 ID 明确拒绝。
- `src/convert_docx.rs`：缓存区分省略和未知 `pStyle`；标题字号、颜色和有效字体按实际段落样式链、字符样式及直接属性计算。按文字脚本拆分不同字体的 run，制表符继承文字标记；引用但无法解析的主题字体发出稳定警告。列表构建后按原段落顺序补回有效段落属性，保留空列表段落间距，不改变编号和嵌套结构。实际 DOCX → IR 回归覆盖段落、标题和嵌套列表的间距继承及直接格式覆盖。
- `src/docx/formatting.rs`：`sz/szCs` 专用精确半点解析；裸整数为半点，`pt/in/cm/mm/pc/pi` 做整数有理数换算。颜色识别 WordprocessingML 主题名称及 `none`，非法值明确拒绝。`rFonts` 的 ascii、hAnsi、eastAsia、cs 四槽及各自主题槽独立继承和解析，引用到不可解析的主题字体不会借用其他槽的旧值。段落间距按 `before`、`after`、`line` 三个分量分别合并，省略继承、显式零覆盖；新的 `line` 与其 `lineRule` 成对覆盖，不能借用上一层的规则。
- `src/docx/mod.rs`：读取 settings 中的 `themeFontLang`，供字体主题槽解析使用。正文、表格单元格及共享块内容解析器保留自闭合 `w:p`，使真实空段落数量、位置及继承间距不丢失；正文继续检查命名空间及 body 边界。
- `src/core/theme.rs`：Word 专用 `resolve_word_opt` 按 HSL 亮度计算 `themeTint/themeShade`；同时读取主题字体方案的脚本补充字体。缺主题或对应色槽时颜色使用已为最终颜色的 `w:val`，主题字体缺项则由导入层提示。

节边界判定依据 Microsoft [MS-DOC PlcfSed](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/68a959a6-11e0-4f3e-9a99-76ca8cc4dddc)：非末节最后一个字符是节结束符，节内其他 0x0C 是手动分页。补丁不从空段落推断分页，不恢复分节版式、自动分页或旧版无结构回退路径的分页。

默认段落样式选择依据 Microsoft [MS-WPE376 §2.1.243](https://download.microsoft.com/download/6/0/E/60EBE400-962D-4B68-92A6-9818644808DA/%5BMS-WPE376%5D.pdf)。字号单位词法参照 [Open XML SDK 的 CT_HpsMeasure 联合类型](https://raw.githubusercontent.com/dotnet/Open-XML-SDK/main/data/schemas/schemas_openxmlformats_org_wordprocessingml_2006_main.json)，半点定义见 [HpsMeasureType.Val](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.hpsmeasuretype.val)。默认字符/表格样式自动应用、完整样式继承及 Word 排版仍未扩展。

段落间距继承参照 [SpacingBetweenLines](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.spacingbetweenlines?view=openxml-3.0.1)。合并覆盖 `docDefaults`、实际默认段落样式（含 Normal）、`basedOn` 链及直接 `pPr`，不会因只设置段后而丢失继承的段前或行距；`line` 单独出现时使用自动行距。此补丁只修复现有表示的合并，不扩展通用数值解析或仅有 `lineRule` 的表示能力。根应用在转换前审核原始 XML，拒绝不能安全编辑的间距单位、自动段距、按行段距及 `lineRule` 单独出现等形式；vendor 的宽松解析不能替代该导入审核。

段落缩进新增按左右分量继承；首行/悬挂作为同一偏移槽覆盖，更近的显式零同样清除继承值。段落/标题的 IR 转换保留 hanging=0。依据 ECMA-376 Part 1 §17.3.1.12、§17.7.2 和 Part 4 §14.3.1.2；`firstLine`/`hanging` 的 ST_TwipsMeasure 是非负源单位（Part 1 §22.9.2.14），有符号编辑模型的负值仅由 hanging 表示。规范下载、版本和 SHA-256 见 [缩进语料清单](../scripts/paragraph-indentation-corpus.json)。独立 LibreOffice 探针验证跨层覆盖/显式零/部分继承；同节点 firstLine+hanging 的消费者差异由根应用拒绝，不声称通用兼容。完整边界见 [PARAGRAPH_INDENTATION.md](../docs/PARAGRAPH_INDENTATION.md)。

Word 主题色依据 Microsoft [MS-OI29500 §2.1.72](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/8229a077-7fc8-4fba-96cc-c77b6a4fc768)：将十六进制值除以 255，shade 使用 `L'=L*shade`，tint 使用 `L'=L*tint+(1-tint)`，保持色相与饱和度。主题名称见 [ST_ThemeColor 对应枚举](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.themecolorvalues?view=openxml-3.0.1)，颜色属性词法参照上述 Open XML SDK schema 的 `CT_Color`。没有可解析主题时，文字中的字面备用色已经包含原主题变换结果。

字体四槽和主题覆盖遵循 Microsoft [rFonts 说明](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/aef3c9a6-5d6c-434b-90b7-85e761fd8e62)；主题语言读取参照 [ThemeFontLanguages](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.themefontlanguages?view=openxml-3.0.1)。导入只保留当前可解析的实际字体名，不保留主题动态关联；语言脚本映射为有限集合。缺失主题、字体方案或被引用的补充字体时，受影响的文字不猜测字体并给出 `DOCX_FONT_THEME_UNRESOLVED` 警告。实际字形依赖本机安装字体与替代机制，尚未做完整 Word/WPS 视觉对照。

主题色导入是近似转换为固定 RGB，不保留动态主题关联。实现以完整精度 HSL 计算后四舍五入到 8 位通道：`4F81BD` 加 `themeTint="99"` 得到规范示例的 `95B3D7`；`C0504D` 加 `themeShade="BF"` 得到 `953735`，与规范中使用近似 HSL 数值展示的 `943634` 各通道相差 1。尚未进行真实 Word 视觉/内部量化对比，不承诺所有 RGB 通道完全一致；自动色或缺失主题且无备用色使用编辑器默认色，并由应用提示。

vendor 主题解析目前只读取 `srgbClr` 和 `sysClr` 的 `lastClr`，并要求可解析的 `fontScheme`；不计算主题 `clrScheme` 内嵌的 DrawingML 颜色变换或其他颜色来源，也没有 `clrSchemeMapping` 设置上下文。根应用导入审计对上述不支持的主题色来源/变换以及显式非默认映射拒绝编辑转换。缺省或显式默认映射允许：`bg1=light1`、`t1=dark1`、`bg2=light2`、`t2=dark2`、`accent1..6` 各自同名、`hyperlink=hyperlink`、`followedHyperlink=followedHyperlink`。默认映射依据 [MS-OI29500 §2.1.395](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/1be23913-3f88-40ab-9cb2-2790be8c549d) 及 [ColorSchemeMapping 属性说明](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.colorschememapping?view=openxml-3.0.1)。

验证命令：

```text
cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --quiet
cargo test --locked
powershell -File scripts/check-corpus.ps1
python scripts/check-corpus-independent.py <上一步输出目录>
git apply --reverse --check --directory=vendor/office_oxide vendor/office_oxide-doc-page-break.patch
git apply --reverse --check --directory=vendor/office_oxide vendor/office_oxide-docx-typography.patch
```

真实语料来源和固定哈希见 `scripts/corpus.json`；它们保存在本地 tmp/corpus，不随本目录分发。验证范围和原生 PDF 证据见 `docs/REGRESSION.md`。
