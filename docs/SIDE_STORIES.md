# DOCX 页眉页脚及附属内容导入保护

## 发现与修复

仅按 `word/header*.xml` 等文件名查找附属内容不足以保证编辑副本完整。OPC 允许页眉页脚关系指向其他名称、目录或扩展名。旧版读取器能把这些文字放入 IR 的独立页眉页脚字段，而简文编辑转换只复制正文；结果是导入成功但另存后附属文字丢失。

本轮仍不提供页眉页脚编辑。审计读取实际主部件的 OPC 关系，按准确的标准关系类型识别 header、footer、footnotes、endnotes、comments；使用与读取器一致的 Strict→Transitional URI 规范化，不以目标路径或后缀猜测类型。主正文中的页眉页脚、脚注、尾注及批注引用也触发拒绝，防止缺失、外部或错类型的关系被解析器跳过。转换层另检查默认、首页、偶数页全部六个 IR 页眉页脚字段，作为第二层保护。

输入文件不会写入；审计和转换仍使用同一有界字节快照。拒绝建立编辑副本时保留原文件，只读预览本身不承诺显示全部附属内容。

## 保守边界

- 未使用的关系、空页眉页脚、只有分隔符的脚注模板部件，以及缺失/外部目标仍可能阻止导入。当前没有计算实际可见性或删除这些声明的许可
- 主 XML 引用检测沿用现有审计/解析器的 local-name 策略；外来命名空间中的同名元素和被忽略的备用内容也可能保守拒绝
- 原有文件名启发式仍保留，因此名字包含 header/footer/comments 等的无关部件可能继续被拒绝；本轮没有收窄历史兼容边界
- 非标准、自定义 URI 恰好以 header 结尾，不会被新增标准关系检查误认。没有标准关系或正文引用的孤立未知部件仍不等于已证明保真；此修复不是完整包内容审计
- 畸形包可能直接报解析错误，不保证都返回同一风险字符串
- 这不是页眉页脚/脚注/批注编辑能力，也不代表 Word/WPS 完整排版兼容

## 复现

```text
cargo test --locked --test word_side_stories
cargo test --locked conversion_refuses_all_six_ir_story_slots_even_if_preflight_misses_them
cargo build --locked
python scripts/check-side-stories.py --cli target/debug/rust-wps --render
```

基线可使用已核对版本 `b2fbcae593c49ca6a2fc87aa1a09c65ce68a5dc6` 的独立 CLI；不要回退当前源码来制造通过结果：

```text
python scripts/check-side-stories.py --cli <基线CLI路径> --expect-baseline-loss --render
```

脚本通过 python-docx 建立并重新读取定向 DOCX 文件，重命名页眉/页脚部件且同步更新关系与内容类型；LibreOffice 渲染为 PDF、pypdf 读取实际显示文字。它们是真实 ZIP/OOXML 文件，但属于定向生成样本，不是用户真实世界语料。默认/首页/偶数页和简单正文对照分别验证拒绝与正常导入导出、重开及原件/已有输出保护。

Rust 六项集成测试覆盖路径/扩展名、三种引用变体、Strict/Transitional、附属关系、缺失/外部/错类型引用和自定义关系对照；一项单元测试直接覆盖全部六个 IR 字段。脚注/尾注/批注用例验证声明审计，不声称这些格式的完整结构读取已经通过。

最新实际结果、证据路径及未执行的桌面/浏览器边界见 [REGRESSION.md](REGRESSION.md)。下一项能力扩展优先做段落间距/行距的编辑、草稿、DOCX 往返闭环，继续按 DOC/DOCX/PDF → Excel → PPT 推进。
