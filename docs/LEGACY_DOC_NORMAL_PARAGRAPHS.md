# 旧二进制 DOC 的编码 Normal 段落身份

本增量只在已有完整 [Latin 字体 / 字号源证明](LEGACY_DOC_FONTS.md) 成功时，保留源文件实际编码的 **Normal 段落身份**。旧读取器按短首行或全大写文字猜出的标题，在编辑副本中恢复为普通段落；没有改动 vendor、只读读取、IR 标题 / 节标题元数据、编辑模型、DOCX writer 或前端。

这是源语义的有界恢复。未标记为标题的 Normal 文字可能具有作者心中的标题用途，本程序不推断这个意图，也不将此次修复称为完整标题样式导入。

## 证明边界与原子修改

`src/legacy_typography.rs` 中的 `Block::NormalParagraph` 明确要求全部活动段落为内建 Normal：PAPX 的 `istd=0`、样式 `sti=0`、paragraph kind、未继承（`istdBase=4095`）、未链接、无修订属性。直接和样式段落 SPRM 仅允许原已审计的间距声明；不能因为某个操作“不影响字体”就将其加入白名单。字体证明以后若扩大到真正标题，必须使用独立身份，不能复用 Normal 变体。

只有同一有界源快照的 FFN / STSH / CLX / CHPX / PAPX、完整来源与精确文字 / 块 / 分页身份全部通过后，候选副本才应用：

- 匹配的顶层 `heading` 变为 `paragraph`，只删除其 `level` 属性
- 其余已有段落属性、根属性、逐字符文字 / 格式标记、颜色、显式字体 / 有效半点字号、真正空段落和独立手动分页保持原位置和身份
- 字体 / 字号仍由前一增量按来源精确拆分恢复；不清除直接粗体等已有字符标记，也不插入空格或清除其他段落属性
- 整个候选模型校验通过才提交，并添加明确的 Normal 身份保留提示；迟到文字 / 分页冲突或候选模型校验失败时，原模型和提示集合均不变

未证明来源仍走原 `Unsupported` 路线，模型和诊断与上一版本相同。包括活动 Heading1–3、继承 / 链接样式、显式大纲级别（包括正文级别 9）、直接粗体、PRM / Prc、未知活动操作、缺失字体表 / 缺省字符样式等；损坏结构和精确身份冲突继续拒绝。名称显示为“Normal”本身不能证明编码身份。

## 真实原件与未修复差异

Apache POI 固定 revision `ae62bb5116b9aee19ebd5834e3a82066132c9f7f` 的 [SampleDoc.doc](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/SampleDoc.doc) 为未改造公开回归原件，27,136 bytes，SHA-256 `717585c0a88f20862c86220163dd03da87cd743970354c6b29add32725f2e833`。未重新分发原件，也不声称已完成单独再分发授权审查。

独立 Python 二进制解码证明其七个 PAPX 均引用 Normal，首行 `I am a test document` 没有源粗体或大纲声明；独立 LibreOffice 转换同样为 Normal。前一字体修复已恢复 Calibri 22 half-points，但仍因标题猜测套用标题粗体及 12 pt 段前间距。本次只消除这一段落身份导致的默认标题效果。最终独立消费者确认首行恢复为常规 Carlito 11 pt，位置与源精确相同；前版本的额外 12 pt 段前空隙已消除。21 对产品输出 / 独立身份-only 参考的完整文字、字体、坐标和光栅严格相等；这不取代下面真实源整页差异的检查。

仍然不保留全部源间距：该源 Normal 的段前 / 段后 / 行距是 `0 / 200 twip / 276`，应用默认 Normal 仍为 `0 / 120 / 360`。源页面、边距、命名样式及动态关系也不在范围内。第二页字面 Arial Black 保留，但固定 LibreOffice 对原 DOC / 最小 DOCX 选择的缺失字体替代仍可能分别为 NotoSans / NotoSerif；`simple.doc` 的 Letter / 90 pt 左边距与应用 A4 / 72 pt 差异仍在。不得用受控参考相等代替真实原件整页保真。

## 验证方法

新增 Rust 单元控制覆盖不同猜测级别、完整适用属性 / 字符标记、空段字体、空段 / 分页顺序、同长度错误文字和边缘空白、迟到冲突的模型 / 提示原子回滚、候选模型失败以及非 Normal / 大纲声明保护。集成控制只对既有独立作者 DOC 的两处正文做同长度修改，不改任何样式或 FKP 边界；同时要求原 vendor IR 的猜测标题 / 元数据保持、编辑模型精确恢复、完整可信 DOCX 重开和原件 / 已有目标不覆盖。

独立验证由冻结前版本输出建立预期，仅对源已独立证明的 Normal 段落删除 Heading `pStyle`，不放宽全部节点类型比较，不从新产品 parser 生成预期。已证明来源的新 / 旧模型差异仅限匹配的 `type` / `level` 和明确诊断；未证明来源与拒绝原因保持。真实原件另作源 / 输出物理字体和字号检查，并保留上述布局 / 替代差异。历史字体-only 的标题粗体失败记录继续保留，本次只明确取代其受证明的 Normal 身份期望。

执行计数、精确命令、CLI / 源码 / checker / 报告身份与独立消费者结果见 [REGRESSION.md](REGRESSION.md)。没有新的实际浏览器、Windows WebView2、原生打印、Microsoft Word 或 WPS 验收。

## 规范锚点

[MS-DOC StdfBase](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/df0f4654-071d-442f-8563-752d7e0285ef)：`sti` 是语言无关的样式身份；`istdBase=4095` 表示未继承。该编码身份与样式显示名称、文字形状、短首行启发式分别处理。
