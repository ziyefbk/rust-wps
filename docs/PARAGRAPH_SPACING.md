# 段落间距与倍数行距

正文、标题及列表/表格单元格内段落可设置段前、段后和倍数行距。工具栏对当前段落或选区内段落生效；显示默认/混合状态，每项可恢复默认。一次操作一次撤销，纳入草稿、DOCX 模型与打印快照。

## 模型与单位

- `spaceBefore`、`spaceAfter`：整数 twip，0–31680（0–1584 pt，0.05 pt 精度）
- `lineSpacing`：整数，120–2400；240 表示 1 行，360 表示 1.5 行
- 这是产品范围，不是 Office 格式的最大值
- 属性仅限 `paragraph` / `heading`；省略表示使用当前 profile 默认值，显式零必须保留。外部 JSON 的 `null`、字符串、非整数和越界值拒绝；编辑器内部的缺省 null 在快照中移除
- 段落间距输入 pt；行距输入倍数，也接受 `241/240` 这样的精确形式。无法有限小数表示的单位用分数显示，避免再次应用改变原值
- 列表编号、起始值、嵌套和缩进不因该操作改变；没有加入段落缩进或表格列宽编辑

## DOCX 读写

显式值写入一个 `w:spacing`，倍数行距使用 `lineRule="auto"`。docDefaults → 段落样式 basedOn → 直接格式按 before/after/line 分量合并，零不会被当成缺省；列表转换保留每项段落的有效属性。实际自闭合空段落也保留，不再被当成缺失内容后补成无样式空段。

外部 DOCX 导入解析有效值后，仍缺省的段前/段后/行距分别实体化为 0/0/240，防止编辑副本的 V2 默认 6 pt / 1.5 行改变源间距。未改旧 DOC 的默认规范化路线，也没有扩大二进制 DOC 版式承诺。其他字体、颜色、页面和复杂样式仍按既有边界处理。

可信自有模型重开继续保留省略属性，不走外部默认实体化。V1/V2 没有任何新间距属性时，历史编码部件保持字节相同；`word-spacing-legacy-parts.json` 固定了两种 profile 的改动前部件哈希。其基线来自 b2fbcae，已核对导出器和锁文件至 c435d02 无变化。

仅含新间距属性的包增加最小 `word/settings.xml` 和正确关系/Content Type。LibreOfficeDev 的受控对照发现：缺少该部件时，相邻段后 12 pt 与段前 12 pt 会相加；加空 settings 部件后取最大值。没有无依据添加兼容模式；新部件纳入哈希、完整集合和重编码校验，删除或同步伪造哈希仍拒绝可信重开。

`read-docx-model <file.docx>` 通过同一验证加载器读取自有模型；与 `import-docx` 外部重新转换分开验证。

## 审计边界

转为编辑副本前，在解析器可能规范化之前检查原始声明：

- 拒绝固定/最小行高、未知 lineRule、单独 lineRule、自动段前/段后、按行计段前/段后、启用的 contextualSpacing、启用或未知文档网格，以及不支持的数值/单位
- 允许规范的关闭自动间距/contextualSpacing；没有 type 或 type=default 的 docGrid 按无网格处理
- 只检查正文使用的段落/表格样式链；无关的未使用模板样式不阻止普通文档，未修改的 python-docx 默认模板有正对照。已用样式链中已被后续覆盖的不支持声明仍可能保守拒绝
- 仅使用中的编号定义/层级会检查；编号层级自身的间距当前不能保留，拒绝转换。正文列表段落上的直接间距及段落样式间距受支持
- 显式表格样式和单元格段落可保留受支持间距；隐式默认表格样式的间距尚未进入转换器，适用时拒绝，未使用默认表格样式不阻止正文文档
- 使用与解析器相同的字面属性身份，防止不同前缀/同 local-name 属性遮盖活动样式；覆盖 Strict XML、历史属性修订、实际 OPC 样式/编号关系及 UTF-16 主部件
- 审计仍不是完整格式安全证明，复杂条件表格样式、兼容设置、分节、分页和所有 Office 行布局不在保真承诺内

## 验证与复现

```text
cargo test --locked --test word_paragraph_spacing
cargo test --locked
cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib
npm run build
npx playwright test tests/ui/word-paragraph-spacing-model.spec.mjs
python scripts/check-word-paragraph-spacing.py --cli target/debug/rust-wps --render --corpus tmp/corpus/SampleDoc.docx
```

独立脚本由 python-docx 生成定向合成包，并使用固定预期清单、独立 OOXML 样式解析和 python-docx 读回；这不是上游真实文档。未经修改的默认模板、继承/部分覆盖/零/边界、标题/列表/表格和不支持形式分开记录。另用已有固定 SHA-256 的 Apache POI 原始文档做正向回归，未把它称为用户生产文件。每次运行使用新目录，核对源哈希、不覆盖及可信自有模型完全相等；外部重新转换的文字色规范化单列。

LibreOffice PDF 探针检查受控 Latin 行基线和相邻段落距离，分别为 1/1.5/2 行与 12 pt 段前/段后/两者同时设置。它只证明 DOCX 消费者路线；不是简文浏览器打印、Windows WebView2、Word 或 WPS 渲染验收。

当前浏览器启动被 socket 权限阻止，Tauri 所需 glib/GTK/WebKit 不完整，Windows 原生环境不可用。已增加但未执行的浏览器用例覆盖焦点/选区、撤销/草稿、导入/导出和打印内联样式；CSS margin collapse、Enter 分段/合并、剪贴板及实际原生 PDF 仍需后续验证。准确本轮计数与证据见 [REGRESSION.md](REGRESSION.md)。
