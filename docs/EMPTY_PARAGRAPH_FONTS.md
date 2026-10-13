# 真正空段落的有限基线字体

DOCX 正文顶层的真正空段落现在可保留有证据的普通 Latin 字体、半点字号、粗体和斜体，避免编辑副本的默认字体改变空行高度。本文中的“真正空段”没有文字、空格、制表符、换行或图片；没有属性 / 子元素 / 文字的空 `w:r` 不改变这个判断。书签 / 校对边界不算可见内容。

这是空行几何的有界恢复。未实现完整 Word 输入样式继承、动态命名样式、多脚本文字行框或任意页面保真。源中的表格 / 列表字体继承、带格式或其他装饰的空 run、空 `w:t`、复杂方向及不能证明的度量仍可能规范化，并明确提示。

## 先分离声明，再选择模型

[Microsoft Learn 的段落标记属性说明](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.paragraphmarkrunproperties?view=openxml-3.0.1) 区分段落标记自身的 `w:pPr/w:rPr` 与内容 run。不能把前一轮单图行框需要专用段落样式的结论直接套用到真正空段。

独立声明隔离的 24 份输入分别测量文档默认、段落样式、段落标记直接格式与空 run：

- DejaVu Sans 12 pt 通过默认 / 样式 / 直接段落标记，两个锚点基线距离均为 28.0 pt；只在空 run 设置该字体仍为 26.65 pt
- 样式 12 pt、直接段落标记 24 pt 得到 41.95 pt；样式 24 pt、直接标记 6 pt 得到 21.0 pt。空 run 的字号覆盖没有这些效果
- Liberation Serif 12 pt 的样式和直接标记均得到 27.8 pt，证明不能只保存字号或把字体族替换成固定名称
- 对既有六表导出只补六处 Aptos 12 pt 段落标记后，表间空段距离从 23.1 pt 恢复到源的 27.4 pt；只改空 run 无效。行高、列宽和颜色没有在这个实验中修改

每份输入后来均以新的 LibreOffice 进程及独立 profile 重新转换，24 组位置全部复现；原共享 profile 批次保留但不单独作为验收依据。证据 `tmp/empty-font-isolation/isolated-report.json`。这是该独立消费者的受控结果，实际字体缺失时仍会替代。

因此本轮直接写入 `w:pPr/w:rPr`，不建立图片专用样式、不修改全局 Normal，也不插入伪空格、零宽字符或空文本 run。

## 模型、源审计与省略契约

可选属性 `attrs.emptyParagraphFont` 仅允许放在 **content 为零个节点**的 paragraph 或 heading：

- 对象必须恰含 `family`、`halfPoints`、`bold`、`italic` 四项
- 字体遵守原有字面名称规则，最多 31 个 UTF-16 单元；字号为 2–3276 整数半点；两个标志必须为布尔值
- 显式 null、部分对象、额外字段、非法类型、错误位置及含任何内容的节点均拒绝；编辑器内部 schema 的默认 null 只在合法节点上从快照移除
- 对已有空节点主动粘贴 / 建模可用于标题、普通列表与单元格；这不等于源表格 / 列表样式继承已经证明
- 不存在该属性时，不改变原 V1/V2 编码。重置移除属性，恢复当前正文 / 标题 profile 默认，而不是恢复源格式

源与转换器共享同一个有界 DOCX 字节快照、实际 OPC 样式 / 主题关系。按 docDefaults、有效段落样式 basedOn 链、段落标记直接属性解析；样式粗体 / 斜体采用 toggle，直接 false 关闭继承。只有一致、可解析的 ascii / hAnsi 字体槽及明确字号才保存；主题 Latin 名称固定化，不保留主题联动。主语言只承诺未声明或 en / es / fr / de / it / pt / nl 的有界 Latin 子集；EastAsia / CS 槽和实际字体回退不在保真承诺内。

活动路径核对默认容器、命名空间、重复属性、样式身份、循环、段落方向、基线对齐及节网格；样式和最终字体解析使用缓存、继承上限与累计工作预算。段落样式 / 默认 pPr 内继承的段落标记属性、标记引用字符样式、字体冲突或缺失、复杂 script / hint / 非零 position / 字符 spacing 不猜测。Word 2010 ligatures 仅在已证明的文档默认 rPr 层被忽略，因为此时没有文字；直接段落标记或活动样式中该扩展仍不在本轮子集。

附加属性前，核对完整正文块顺序、周围文字、逐表 / 行 / 格的段落文字与空段身份。单独手动分页映射为 pageBreak；lastRenderedPageBreak 仅是源保存时的排版位置提示，不冒充用户手动分页。图片占位段仍由原媒体 / 图片审计移除，真正空段不合并。只靠空段计数或遍历顺序不能产生保留声明。整份块身份核对失败或字体声明解析有错误时，不附加部分结果，保留既有导入路径并增加明确规范化提示。身份完整、但某个空段缺少可确定的字体 / 字号时，只跳过该段；其他已独立证明的空段仍可保留，提示会同时说明有未证明空段。不能由“部分保留”推断所有空段都已恢复；原有阻断项不放宽。

## 编辑行为

光标位于带此属性的空段时，界面显示源空段字体与“默认空段字体”重置入口。上方普通字体 / 字号控件仍设置后续文字。编辑与不可变打印快照使用同一安全字面 CSS 字体声明，不生成占位文字。

- 首次输入、粘贴非空内容或组合输入提交时，在同一个根事务里移除此空行覆盖；新文字继续使用当前文字 marks / profile，不偷偷套用源空段字体
- 空段直接切换正文 / 标题保留此显式覆盖；撤销 / 重做、草稿和可信 DOCX 重开保留它
- 分段 / 合段不把源覆盖传播到新节点；删除文字后再次为空时，不复活已经结束的源覆盖
- 合法空段粘贴可保留规范元数据；错误 / 非空 / 错位元数据先验证，再进入 schema。失败候选、预算超限、锁定或过期选区不会提前清除原属性
- 重置与首次输入各按正常事务历史恢复，段落间距和图片格式不受重置影响

## 未改造源与复验

两份 Apache POI 原件固定于 revision `ae62bb5116b9aee19ebd5834e3a82066132c9f7f`，不为通过检查而删除声明：

- [table-alignment.docx](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/table-alignment.docx)：SHA-256 `ee36729e0d050e529fea0b444006a194bea72b35cf8958ae58c6624c26156b41`，六个 Aptos 12 pt 空段。行高修复与空段修复分别测量；自动宽度、橙色表头 / 白字等仍有既有规范化
- [NoHeadFoot.docx](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/NoHeadFoot.docx)：SHA-256 `dc6a67b1c118658acadc87ba41200f0b90ead05ada95e7ce6fa72a97700430c5`，四个 Calibri 11 pt 真正空段；第九个正文段的手动分页不是空段字体目标

另保留原上下文间距的受控反例 `empty-neighbor-explicit-reference.docx`，SHA-256 `34f60b81f669e29932c8ec04223d5f71bf828bc7d491e26429ef8dfb4a176ff5`。该段含一个 bare `w:r`，不含文字；将其误称完全无 run 会掩盖源身份。它与独立新创作的无 run 对照分别登记。

复验入口：`cargo test --locked --test word_empty_paragraph_fonts`；`python scripts/check-empty-paragraph-fonts.py --help`。最终命令、冻结 CLI / 报告散列、通过计数和历史整包字节回归见 [REGRESSION.md](REGRESSION.md)。浏览器、实际中文 IME、系统剪贴板、原生 WebView / PDF 仍未执行；无浏览器插件 / DOM 检查和 LibreOffice PDF 都不能替代这些验收。
