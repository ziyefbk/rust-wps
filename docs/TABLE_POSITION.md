# 简单表格的对齐与左缩进

支持简单、未合并、非嵌套矩形表格的整体左对齐、居中、右对齐，以及有限左缩进。它们与单元格内的段落对齐独立。选中跨表内容时可显示混合值并原子修改；工具栏、撤销/重做、草稿、DOCX 另存及可信模型重开保留属性。

## 模型、默认值与边界

- 可选 `table.attrs.tableAlign` 为 `left`、`center`、`right`；省略表示默认左对齐
- 可选 `table.attrs.tableIndentTwips` 为整数 twip，界面以 pt 输入，精度 0.05 pt。省略与显式 `0` **不同**，不能互相替换
- 固定列宽总和（自动表格使用 9000 twip 的作者默认宽度）加缩进不超过 31680 twip。这是资源/数值边界，不是页面适配保证
- 正缩进只允许左对齐或默认对齐。选择居中/右对齐会原子清除正缩进；显式零保留。对居中/右对齐应用正缩进会拒绝，须先改为左对齐
- 不接受 null、字符串、布尔、浮点、负数或超限外部模型。新增属性不能出现在其他节点或任何嵌套表格组合；无新属性的旧嵌套模型不扩大或缩小历史范围
- 默认对齐只清除对齐属性；空缩进输入清除缩进属性；“默认位置”清除二者。失败不修改正文、草稿或撤销历史

## 缩进为什么不是简单的边框偏移

本应用保留既有 DOCX 兼容基线，不为一次表格编辑全局切换 Word compatibilityMode。独立 LibreOffice 受控探针表明：在该基线下，显式左缩进以首列文字边缘为参照，默认水平单元格边距为 108 twip（5.4 pt）。因此显式缩进 0 的左边框约比默认位置左移 5.4 pt；缩进 36 pt 时边框约右移 30.6 pt。省略缩进不做这个补偿。居中/右对齐的零缩进不做左偏移。

新增位置元数据的 DOCX 表格显式写入水平单元格边距 108 twip，编辑与打印快照声明对应的水平内边距和表格偏移。没有位置属性的表格继续使用原来的编码和 CSS。设置了位置但仍自动宽度的表格以 450 pt 为作者默认宽度，仍是自动布局，内容可能撑宽；未设置位置的自动表格继续旧宽度行为。

这是有界实现，不是 Word/WPS 完整布局保证。无显式页面属性的旧模型允许表格超出正文区；CSS 自动边距与 DOCX 对超宽居中/右对齐的处理可能不同，没有声明溢出版式等价。真实浏览器、Windows WebView2 和本应用原生打印尚未验证。

## 外部 DOCX 的安全边界

几何审计、OPC main/styles/settings 解析和正文转换使用同一有界文件快照。表格数量、顺序及每行单元格形状继续交叉核对；另存禁止覆盖源文件或已有目标。

- 直接、唯一的左/中/右对齐可保留；在禁止 `bidiVisual` 的 LTR 子集中，`start`/`end` 规范化为左/右。段落 `jc` 不误当表格对齐
- 直接缩进仅支持非负整数 `dxa`。负数、百分比、auto、nil、缺少单位、冲突命名空间、未知属性、重复或包装在错误位置的声明拒绝
- 有直接缩进（包括零）的表格只接受缺省或已验证的旧兼容模式 11/12/14；15、未知模式、重复或不明确的 settings 关系/声明拒绝。Word 2013+ 的边框参照与旧模式不同，不能仅复制同一个整数就宣称位置保持
- 缩进路径要求声明的水平表格/单元格边距为 108 twip，包括活动样式链；其他值、逻辑/物理别名冲突拒绝。上/下边距和其他单元格外观仍按既有提示规范化
- 居中/右对齐加正缩进拒绝：ISO/LibreOffice 忽略它，而 Microsoft 文档描述 Word 有不同处理；不猜测目标行为
- 独立探针区分隐式默认样式零缩进（保持省略）和显式选择样式链的零缩进（旧兼容模式下物化为显式零）。后者同样须通过 settings/边距证明；固定表格证明失败则拒绝，自动表格且没有直接缩进时保留旧规范化路线并明确警告继承缩进不保真。直接缩进绝不静默降级；条件样式缩进不能当作无条件属性
- 活动或默认样式链中的非中性定位、条件几何或无法解析的继承保守拒绝；无关未使用样式不阻止导入
- 固定列宽仍须满足 [固定列宽](TABLE_WIDTHS.md) 的全部证据；普通自动表格保留受支持对齐属性，但列宽继续明确规范化，不把它称作绝对位置/宽度的保真往返

## 验证与真实语料

`scripts/check-word-table-position.py` 使用 python-docx 自制定向文件、固定预期清单及独立 XML/python-docx 读取器；分别验证导入、未修改/实际位置编辑后的导出、完整可信模型相等、非法候选拒绝、原件哈希和禁止覆盖。受控 LibreOffice PDF 检查页面尺寸、表格边框和文字边缘，不冒称本应用/Word/WPS 渲染验收。

未经改造的 Apache POI 原件另列来源、revision、SHA-256 和 Git blob：七张固定表格的 `table-indent.docx` 仍明确拒绝（现代兼容模式、自定义边距、不同不支持单位及负缩进）；六张自动表格的 `table-alignment.docx` 用于受支持对齐属性及宽度规范化的真实正对照。两者均不称作用户生产文件，没有提交二进制；再分发权与原始作者仍未全面审查。

复现命令：

```text
cargo test --locked --test word_table_position --test word_table_import --test word_table_widths
npx playwright test tests/ui/word-table-position-model.spec.mjs
python scripts/check-word-table-position.py --help
```

最终计数、证据路径、回归和实际未执行项见 [REGRESSION.md](REGRESSION.md)。没有新增属性时，V1/V2 历史包逐部件字节不变；vendor 和锁文件未修改。

### 标准说明来源

[Microsoft TableIndentation](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.tableindentation?view=openxml-3.0.1)、[Word 对非左对齐缩进的差异](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/a129dffd-ef75-4faa-ab1f-47c3fd4cff26) 和 [TableJustification](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.tablejustification?view=openxml-3.0.1) 用于确定标准/消费者边界。compatibilityMode 与隐式/显式样式零缩进的实际距离结论来自报告中的受控实验，不把单个消费者的实验当作所有 Word/WPS 版本行为。

显式单节页面的尺寸/边距及对象适配限制另见 [PAGE_GEOMETRY.md](PAGE_GEOMETRY.md)。无页面属性保留本文件所述历史行为；有页面属性时新增正文区检查，不自动缩放图片或固定/定位表格；未定位自动表格按正文区明确规范化。
