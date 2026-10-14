# DOCX 主题颜色的消费者校准

本轮纠正一项验证结论，不修改产品颜色算法。此前两份真实原件及一个继承控制的源 / 输出光栅不相等是真实结果；将其直接归因为 HSL 舍入尚无充分证据。当前固定版本 LibreOffice 在这些 WordprocessingML 路径中消费 `w:val` 的字面 RGB，而应用根据可解析主题与变换计算 RGB。缓存颜色不是独立的主题变换计算器。

## 规范、实现与限制

- ECMA-376 Part 1 §17.3.2.6 指定：存在 `themeColor` 时，运行文字的 `val` 被主题色取代。规范包的固定来源及 SHA-256 见 [缩进来源清单](../scripts/paragraph-indentation-corpus.json)。不能为迎合一个消费者的页面图像，把可解析主题降到字面备用色之后
- Microsoft [MS-OI29500 §2.1.72](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/8229a077-7fc8-4fba-96cc-c77b6a4fc768) 记录 Word 的 HSL 亮度变换：十六进制值除以 255；shade 为 `L'=L*s`，tint 为 `L'=L*t+(1-t)`，tint 优先于 shade。该说明没有给出足以确定所有 8 位输出的量化步骤，不能从一个近似示例反推通用舍入规则
- 当前应用保留完整精度 HSL，最后通道四舍五入；主题不存在或色槽不可解析才使用已为最终 RGB 的字面备用值，不重复施加变换。显式 RGB、其他格式通用颜色解析器、主题映射 / 复杂 DrawingML 变换拒绝边界均未改动
- 固定消费者为 LibreOfficeDev `26.8.0.0.alpha0`，revision `2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`。其 [ThemeColorHandler.hxx](https://github.com/LibreOffice/core/blob/2c87e51eeaa2b413ff4ae097b2705eea1995d8e5/sw/source/writerfilter/dmapper/ThemeColorHandler.hxx) 中 `mnColor` 初始为 0，只有 `w:val` 为其赋值；主题、tint、shade 单独存储。[DomainMapper.cxx](https://github.com/LibreOffice/core/blob/2c87e51eeaa2b413ff4ae097b2705eea1995d8e5/sw/source/writerfilter/dmapper/DomainMapper.cxx) 的 `LN_EG_RPrBase_color` 分支先将 `mnColor` 设为字符 RGB，另保留复杂颜色元数据。这与本轮实际消费结果相符，不外推为全部 LibreOffice 功能或版本的行为

## 独立算术不能被页面缓存替代

独立有理数计算与 Python `colorsys` 分别核对 HSL 变换，不调用产品实现：

| 输入 | 完整精度通道约值 | 最终四舍五入 | 直接向下取整 | 示例或原件缓存 |
| --- | --- | --- | --- | --- |
| `4F81BD` / shade `BF` | 54.746524 / 96.221164 / 145.990731 | `376092` | `366091` | `365F91` |
| `C0504D` / shade `BF` | 148.815589 / 55.178814 / 52.670686 | `953735` | `943734` | MS 示例 `943634` |
| `4F81BD` / tint `99` | 149.4 / 179.4 / 215.4 | `95B3D7` | `95B3D7` | MS 示例 `95B3D7` |

普通 floor 甚至不能得到待解释的 shade 值；没有新增每通道偏移、特例颜色或按消费者调参。MS shade 示例展示的 HSL 本身已经近似，实际 Word/WPS 内部量化和可见结果仍未验收。保留近似固定 RGB 的既有提示，不宣称所有通道完全兼容。

## 受控消费者证据

[独立 checker](../scripts/check-docx-theme-consumer-probe.py) 用 python-docx / 直接 OOXML 构造一份 16 行矩阵，并对真实 `Styles.docx` 生成两个明确标注的单属性派生控制：

1. 相同主题与 shade，分别写入原蓝色、绿色、洋红色和省略的 `w:val`
2. MS shade / tint 示例、tint 与 shade 同时存在、无变换主题、显式 RGB 及没有主题的 shade 属性
3. 真实原件只把活动 Heading1 的 `w:val` 从 `365F91` 改为 `FF00FF`；其他 XML 字节和包内其他部件保持
4. 另一派生控制只把同一 Heading1 的 `themeShade` 从 `BF` 改为 `80`，字面颜色保持；其他部件保持

16 行消费者 RGB 全部跟随 `w:val`，所有无 `val` 的主题行均为黑色。真实 cache-only 派生使 Heading1 变为洋红色；shade-only 派生仍为 `365F91`。两组派生的完整文字、字体属性和坐标与原件精确相同。应用相反地保持 theme 优先：cache-only 模型完全相同，shade-only 模型只将 Heading1 色改为 `254062`。这些事实区分了缓存读取和主题计算，而不是证明某种舍入修复。

真实源为 Apache POI 的公开回归夹具，revision `ae62bb5116b9aee19ebd5834e3a82066132c9f7f`，不是用户生产文档。`Styles.docx` SHA-256 为 `63f3fe4c3b4fbdc211eca8fa7fd31a799ff4438afbf4ca6e771b3aef107c732e`；原件从未改写。`NoHeadFoot.docx` SHA-256 为 `dc6a67b1c118658acadc87ba41200f0b90ead05ada95e7ce6fa72a97700430c5`，本轮核对其原件身份，沿用前轮源 / 输出失败证据，未把它计作本轮新渲染。派生控制不冒充新真实来源；二进制只用于本地验证，不新增再分发授权。

每个 PDF 使用独立的新 profile / HOME，保存完整命令、输入 / 输出 / CLI / 消费者哈希、逐 span RGB / 字体 / 坐标和实际 PNG。没有降低历史像素阈值，也没有将三个旧 `strictly_equal=false` 改成通过。准确报告身份与复现命令见 [REGRESSION.md](REGRESSION.md)。

## 后续边界

继续保留主题优先级与现有近似 HSL 路径。本次只有文档和独立校准工具变化，不是新增颜色编辑能力，也没有关闭三组历史源 / 输出光栅差异。后续若要改变 8 位量化，必须有独立的 Microsoft Word/WPS 变换证据或更明确的算法来源，并覆盖缓存冲突、缺失备用值、tint 优先级及无关颜色回归。实际浏览器、WebView2、系统打印与完整颜色管理仍未验收。
