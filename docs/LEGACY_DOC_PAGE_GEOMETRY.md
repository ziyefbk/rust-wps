# 旧二进制 DOC 的单节页面几何

后续 [独立分页段增量](LEGACY_DOC_BREAK_PARAGRAPHS.md) 在明确 DOP / break / CR / 节界的完整证明下保留 split 语义与分页段 metrics。本文的分页段纵向差属于历史记录；字体替代、空段基线及完整版式限制仍保留。

本增量只在现有完整 [Latin 字体 / 编码 Normal](LEGACY_DOC_NORMAL_PARAGRAPHS.md) 正文证明内，保留一节文档的纸张宽高和四边正文边距。使用现有 `pageGeometry` 六整数模型和 writer，不修改 vendor、普通 IR 转换、分页原子、页面 schema、UI 或兼容模式。它恢复编码值，不是正文物理行框或完整 Word / WPS 版式证明。

## 支持子集

- 经过原有有界 CFB / FIB / FFN / STSH / CLX / CHPX / PAPX 和精确正文 / 段落 / 手动分页身份核对的 Word97 / Word2007 Latin Normal 正文
- 唯一的 PlcfSed 节范围为 `CP 0..ccpText`；SEPX 明确给出纸张宽高和四边边距，不使用安装语言、编辑器或消费者猜测的默认值
- 纸张仍为既有 A4 / Letter / A5 横纵向子集，每边允许名义值 ±2 twip；直接保存源整数，不吸附预设、不经浮点换算。完整模型约束见 [PAGE_GEOMETRY.md](PAGE_GEOMETRY.md)
- 缺省方向为规范的纵向；显式方向只接受 portrait=1 / landscape=2，并与宽高方向一致。四边非负且各不超过 4320 twip，正文宽高各至少 1440 twip
- 只有中性的单栏、关闭正文网格、顶部对齐、从左到右、无装订线和无特殊首页布局进入新证明
- `SDyaLinePitch` 或 `SDxtCharSpace` 本身不启用网格，是否启用由最终 `SClm` 决定。单栏时的合法列间距同样不启用多栏
- 无附属故事 / 页眉页脚记录时，header/footer 距离不改变这六个正文几何整数；纸张选择 tie-breaker 和 revision save ID 也不作为可见页面属性保存

正的上下边距在 DOC 中是会为页眉 / 页脚 / 脚注让位的最小值，因此只有证明这些附属内容不存在时才进入本子集。负值是另一种固定边距语义；即使绝对值在编辑器范围内，本轮也不转换它。

## 证明、回退与拒绝

几何与字体证明分别分类。完整字体 / Normal 证明为 Unsupported 时，原整份响应保持，不借本增量增加几何能力或改变该路线的诊断。对于已证明正文：

1. 几何证据完整且支持：在精确正文绑定与字体 / Normal / 间距修改成功后，只向候选根节点添加 `pageGeometry`。完整模型再次验证后提交；更新原有“节布局不保留”提示为仅保留受审计单节几何，并保留所有其他限制提示
2. 缺失证据或合法但超出子集：只增加具体的页面规范化提示，完整旧模型、字体 / Normal / 间距和分页顺序保持。包括缺失 DOP / PlcfSed、历史 `fcSepx=FFFFFFFF`、缺失任何几何分量、多节、合法纸张或边距超出模型、方向矛盾、活动网格 / 多栏、负固定边距、未知合法 framing 的操作等
3. 已声明且损坏：长度 / 偏移 / CP 单调 / 覆盖 / 已证明分节标记错误、负 SEPX 长度或非 sentinel 负偏移、已知标量非法值、DOP 版本与长度错配、已定义兼容字段副本冲突，或没有 header text 却声明页眉页脚记录拒绝导入。即使另一个特性未证明，或随后被合法值覆盖，非法声明仍拒绝

缺失 DOP、历史 `FFFFFFFF` sentinel 和缺失必需边距的回退是明确的既有兼容路线，不把它们说成规范完备文档。合法的末尾节 CP 可以大于正文终点；本轮将这种情况列为未证明，而不是损坏。

已知标量按最后声明生效，不同时重置其他分量。`SWall=1` 后的 `SWall=0` 按规范撤销保留修订状态；非法 Bool8 不能被后来零值隐藏。未知操作只做有界 framing 并阻止几何保留，不宣称已实现全部二进制语义。多节全部 SEPX 也检查 framing 和已知标量；因为多节始终未证明，仅从已证明正文块累计 CP 核对实际分节标记，没有增加列宽覆盖或其他完整跨属性语义解析器。

导入过程只使用同一份最多 64 MiB 的快照和已核对身份的表流。新增页属性预算独立限制为累计 1 MiB、最多 10,000 个节和 10,000 条操作；SEPX 长度首先按有符号 16 位验证。所有读取检查边界。零长度表记录的偏移、SED 的 fn / fnMpr / fcMpr 按规范忽略，真实原件的非零忽略字段不会误触发拒绝。

## 文档级影响因素

按有效 FIB 版本识别 DOP，不仅看基区 `nFib`：Word97 的 Dop97 为 500 bytes；有效 Word2007 布局允许 674 / 690 / 694 bytes，分别包含后续 DOP 扩展。非零但不匹配的长度拒绝；未声明则保留几何回退。

以下活动设置不进入新保留子集：FIB 安装语言页面覆盖、不同奇偶页页眉、镜像边距、顶部装订线、两页合一、书籍折页 / 反向折页、阅读页面锁定、打印机度量，以及 FIB 声明的打印机或页眉页脚记录。反向折页没有同时启用折页属于损坏。DOP 重复的 Copts60 / Copts80 已定义字段必须一致；Copts60 bit9 属于规范要求忽略的位，差异不误拒绝。

未启用阅读页面锁定时，后续 DOP 的 actual-page 阅读偏好不覆盖物理页面，不单独阻止证明。其他文字兼容选项、默认空段字体、分页段间距、自动分页与字体替代仍不在此几何证明内，不能据六个数值相等宣称物理排版相同。

## 未改造真实原件

Apache POI 固定 revision `ae62bb5116b9aee19ebd5834e3a82066132c9f7f` 的公开回归文件。原件未改写或提交，不声称已完成单独再分发授权审查。

- [simple.doc](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/simple.doc)，SHA-256 `4876e828e62d490fa188c7c4e47013e2e0dfdf02f5903dfd374bc0d2b2049ca1`：`CP 0..48`，SEPX 在 WordDocument 3072；Letter 12240×15840 twip，左右 1800、上下 1440、装订线 0
- [SampleDoc.doc](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/SampleDoc.doc)，SHA-256 `717585c0a88f20862c86220163dd03da87cd743970354c6b29add32725f2e833`：`CP 0..138`，SEPX 在 WordDocument 3584；11907×16840 twip，四边 1440、装订线 0。编码 linePitch=360 未启用网格

独立二进制读取的整数是源 oracle。固定 LibreOffice 将 SampleDoc 转为 DOCX 时纸张变成 11906×16838；不能据此把源值改成消费者舍入值，也不能提高容差隐藏区别。历史字体 / Normal / 间距轮的原始失败记录保留，本轮只取代本证明覆盖的页面数值期望。

## 验证与限制

Rust 测试覆盖精确整数 / 横纵向、最后优先、缺证据回退、全部相关 DOP flag、版本长度、未知及截断操作、覆盖前损坏、CP / SEPX / 表边界、资源上限和完整模型 / 诊断原子回滚。独立作者二进制与真实原件分列；独立检查必须核对完整模型，仅允许受证明根几何和精确提示变化，另核对实际 DOCX `sectPr`、可信重开、源哈希、不覆盖和定向篡改拒绝。

最终独立检查完成 306 输入：34 支持、102 几何专属回退、28 原样 Unsupported、86 既有拒绝与 56 新损坏拒绝；完整模型、306 次只读相等 / 可信重开、612 次不覆盖、34 份 XML 几何 oracle 和 20 个定向篡改拒绝通过。31 份新 PDF 使用逐文件隔离 LibreOffice：12 对产品 / 独立参考逐字符、坐标与完整光栅精确相同；另与两份真实 DOC 和三份独立作者二进制源分别比较。

真实 simple 的 Letter 612×792 pt、首字 x≈90.1 pt 与整页光栅现和源精确相同；SampleDoc 源 11907×16840 twip 仍原样保存，第一页字符 / 光栅与源相同。固定消费者对 A4-like 纸张的 MediaBox 舍入没有改写这些源整数。SampleDoc 第二页缺失字体替代及独立分页段的纵向差仍存在，不能称整份真实文件完整保真。

另保留上轮独立作者缺 DOP（`lcbDop=0`）控制的 12 pt 相邻段距差异：在该固定消费者中，缺失 / 历史 DOP 默认路径采用相加，而产品 DOCX 采用取最大值。这不是所有 DOC 对 DOCX 的固有规则，也不是显式编码兼容 flag 的源证明；本轮没有解决该差异或改写历史失败记录。

消费者比较使用独立参考和隔离 LibreOffice 进程，同时保留真实源文件比较；产品与参考相同不能替代源版式相同。最终实际执行计数、固定 CLI / checker / 报告身份和未执行项记入 [REGRESSION.md](REGRESSION.md)。本轮未新增实际浏览器、Windows WebView2、系统打印、Microsoft Word 或 WPS 验收。

## 规范锚点

- [Section properties](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/46c3ec54-53ff-4c0a-b0d6-07ad15d2546e)、[SBOrientationOperand](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/f35a0396-e4d0-4d8e-8b7f-6114d01b864a)、[SClmOperand](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/06301327-0f19-454e-a117-552725a86c1d)
- [PlcfSed](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/68a959a6-11e0-4f3e-9a99-76ca8cc4dddc)、[Sed](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/ae1ec5fc-e3c0-4e27-956e-9ceedc41cc2a)、[Sepx](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/1cc8d6e1-17e2-4667-99b6-39b2c70a0ebe)
- [FibBase](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/26fb6c06-4e5c-4778-ab4e-edbf26a545bb)、[Dop versions](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/841b5f72-487e-4fe7-8657-ec90d5af8750)
- [DopBase](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/f885b87a-15b3-460e-aecc-213bd17f960e)、[Dop97](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/2feeb178-f901-4151-bf5c-8496d5a0700b)、[DopTypography](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/d6676a31-1afb-40c8-a5c1-745f4e632c87)
- [Dop2002](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/4ba592e9-dff6-466e-b13d-257c325c4d8a)、[Dop2003](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/8783a51d-da22-4e5f-a8f0-d98ae25215c5)、[Dop2007](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/a97f1d2f-b260-4c55-9984-9f08021d1db5)、[Copts80](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/6ada042e-d11e-4c6f-9c7e-6cc27281ed3d)
