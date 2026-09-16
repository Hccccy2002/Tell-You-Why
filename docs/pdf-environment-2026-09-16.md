# PDF 运行环境配置验收（2026-09-16）

## 问题与处理

用户在 Tell You Why 的“PDF 知识库”页面看到“本机 PDF 处理组件未就绪。请按项目 README 安装 Python 运行环境后重试”。实际项目为 `D:\Tell-You-Why`。源码 `src-tauri/src/knowledge_base.rs` 默认寻找 `rag-service/.venv/Scripts/python.exe`，本机原来没有该虚拟环境，默认 Conda Python 为 3.14.6，超出模块声明的 `>=3.11,<3.13` 范围。

本次使用已有的 `C:\Users\test\miniconda3\envs\cjj\python.exe`（3.12.13 x64）创建项目独立 venv，安装锁文件中的 100 项依赖和 `tellwhy-kb` 本地包；没有修改该 Conda 环境中的包。基础 Python 仍是 venv 的运行依赖。原开发记录中的 3.12.14 保留为历史环境，不冒充本机实测版本。

已准备项目固定 revision 的 OCR 检测、识别、版面分析、BGE Embedding 和 BGE Reranker，模型目录约 1.41 GB。原数据目录没有已发布教材知识库，本次没有自动导入整本教材。

## 真实检查结果

| 检查                    | 结果                                                              | 证据                                                                       |
| ----------------------- | ----------------------------------------------------------------- | -------------------------------------------------------------------------- |
| 锁定依赖                | 100 项版本逐项一致，pip check 无冲突                              | [运行报告](validation/pdf-environment-20260916/runtime-smoke.json)         |
| 基础模型                | det / rec / layout / embedding 文件哈希校验通过                   | [模型清单](validation/pdf-environment-20260916/model-manifest.json)        |
| 重排模型                | 固定 revision、全部文件校验通过                                   | [重排清单](validation/pdf-environment-20260916/reranker-manifest.json)     |
| Python 回归             | 69 passed，0 failed，53.66 秒                                     | [JUnit 结果](validation/pdf-environment-20260916/pytest-final.xml)         |
| PDF 读取与渲染          | 合成 PDF 1 页，能提取文本并渲染 1190 × 1684 图片                  | [运行报告](validation/pdf-environment-20260916/runtime-smoke.json)         |
| OCR 真实推理            | 识别 CPU / Memory 等测试句，得到 5 个块，约 4.84 秒               | [运行报告](validation/pdf-environment-20260916/runtime-smoke.json)         |
| Embedding 真实推理      | 2 × 512 维，有限值且单位范数                                      | [运行报告](validation/pdf-environment-20260916/runtime-smoke.json)         |
| Reranker 真实推理       | 相关文本得分 6.0684，无关文本 -10.1933；离线执行                  | [重排报告](validation/pdf-environment-20260916/reranker-smoke.json)        |
| APP 使用的 desktop 协议 | catalog 返回 models_ready=true；inspect 正确返回测试 PDF 为 1 页  | [协议结果](validation/pdf-environment-20260916/desktop-adapter-smoke.json) |
| 真实 APP 页面           | Python 组件及模型缺失提示消失，文件选择器可以打开，已取消回到页面 | [页面截图](validation/pdf-environment-20260916/app-ready.png)              |

OCR 样本仍被质量规则标记为 `needs_review`，没有将其改成已人工验证的文档。上述 OCR 检查证明运行时可以执行并识别已知测试文字，不证明任意教材全文识别质量。重排的两段短文本检查也不代替检索质量评测。

本次只做本地环境和离线模型检查，没有请求 DeepSeek / Kimi API。没有重跑前端或 Rust 全套测试，没有生成新的 Release 或安装包。

## 失败与限制

首次 Python 测试为 25 passed、44 errors，错误均发生在测试夹具初始化：已有 `%TEMP%\pytest-of-test` 目录拒绝访问。原始结果保留在 [首次 JUnit](validation/pdf-environment-20260916/pytest.xml)。改用项目 `tmp/` 下新建的唯一 `--basetemp` 后，69 项全部通过；没有修改系统目录 ACL，没有删除旧测试目录。

文件选择器的自动化检查遇到控件缓存索引失效，未将“通过 UI 选中并导入文件”记为通过；已取消选择器。PDF 页数和读取通过与 APP 相同的 Python desktop 协议检查，真实 APP 页面已确认环境错误消失。

## 复现

项目根目录运行：

```powershell
.\rag-service\.venv\Scripts\python.exe --version
.\rag-service\.venv\Scripts\python.exe -m pip check
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb models verify --data-root .\data
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb.indexing.reranker verify --models-root .\data\models

New-Item -ItemType Directory -Force -Path .\tmp | Out-Null
$testTemp = Join-Path (Get-Location).Path ('tmp\pytest-pdf-' + [Guid]::NewGuid().ToString('N'))
.\rag-service\.venv\Scripts\python.exe -m pytest .\rag-service\tests -q --basetemp $testTemp
```

[环境摘要](validation/pdf-environment-20260916/summary.json)保留版本、安装位置、锁文件哈希与验证边界；[运行样本脚本](validation/pdf-environment-20260916/runtime_smoke.py)用于本机合成 PDF 的 OCR 和向量计算复现，产物写到 `tmp/pdf-environment-20260916/`。

根 README、Python 模块 README 和 PDF 桌面指南已同步更新：直接提供安装步骤、Conda/Python Launcher 两种创建方式、模型准备、实际路径、部署边界及常见错误。已有的 `src-tauri/Cargo.toml` 工作区修改未改动。
