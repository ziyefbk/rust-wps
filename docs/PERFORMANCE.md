# Rust 核心性能基线

环境：Windows 11 26200、16 逻辑 CPU、rustc 1.92.0，`cargo build --release --locked`。原始五次结果、时间、二进制/锁文件/输入 SHA-256 见 [performance-baseline.json](performance-baseline.json)。

每次启动一个新的 CLI 进程；表中耗时为第 2–5 次进程总耗时的中位数，包含进程启动、核心操作及 JSON 输出。操作系统缓存未控制，第一次也不等于冷启动。`inspection_ms` 是 Rust 检查入口自身的整数毫秒耗时，包含读取/检查/提取，不能当成纯解析器耗时；小文件出现 0 表示精度不足。导入没有内部耗时字段。

| 输入 | 操作 | 重复运行中位总耗时 ms | 五次最大峰值工作集 MiB |
|---|---|---:|---:|
| POI simple.doc | 读取 | 34.3 | 5.01 |
| POI simple.doc | 导入 | 71.4 | 5.21 |
| POI SampleDoc.doc | 读取 | 71.1 | 5.00 |
| POI SampleDoc.doc | 导入 | 37.8 | 5.22 |
| POI SampleDoc.docx | 读取 | 41.0 | 5.66 |
| POI SampleDoc.docx | 导入 | 47.4 | 5.97 |
| 合成 1,000 段 DOCX | 读取 | 37.2 | 5.84 |
| 合成 1,000 段 DOCX | 导入 | 72.3 | 7.77 |
| 合成 4,000 段 DOCX | 读取 | 48.4 | 8.30 |
| 合成 4,000 段 DOCX | 导入 | 89.9 | 15.16 |
| 合成 100 页文字 PDF | 提取 | 62.8 | 5.38 |

内存取 Windows [PROCESS_MEMORY_COUNTERS](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters) 的 PeakWorkingSetSize，另记录 PeakPagefileUsage（峰值提交内存）。它们与桌面 worker 的 512 MiB 提交内存限制是不同指标。不是采样 RSS，也不是 WebView2 或整款应用的内存。

复现：先运行 `scripts/check-corpus.ps1` 准备固定版本语料，再执行 `cargo build --release --locked`，用带 pypdf 的 Python 运行 `scripts/measure-core.py`。脚本在 `tmp/benchmarks/` 创建独立输出，生成简单 DOCX/PDF，校验首尾文字标记、真实语料哈希及运行前后源字节。每个操作运行五次、单次 30 秒截止；失败不会生成成功报告。

这轮测量发现中英文混排被上游拆成多个字体片段，4,000 段导入因此误触节点上限。导入现合并相邻且基本样式/链接相同的文本节点，保持段落、换行和不同样式边界；新增逐段文字一致性回归。

这些简单结构支持继续开发 Rust 核心，但不能证明复杂 Word 保真、大图片/扫描 PDF 性能、桌面启动速度或 UI 大文档流畅度。五次运行不足以建立稳定的尾延迟或硬性产品指标；仍需更大真实语料和桌面测量。
