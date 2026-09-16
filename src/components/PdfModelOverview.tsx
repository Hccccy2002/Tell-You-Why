import { useState } from "react";
import { friendlyError, isDesktop, openSourceUrl } from "../lib/api";

// Names and download revisions match tellwhy_kb/models.py and indexing/reranker.py.
const modelGroups = [
  {
    title: "文档识别",
    description: "用于 PDF 导入时的文字识别与版面分析。",
    models: [
      {
        role: "文字检测",
        name: "PP-OCRv5_server_det",
        downloadUrl:
          "https://huggingface.co/PaddlePaddle/PP-OCRv5_server_det/tree/ca867c897ecbca8873081573a802ad70d499cb94",
        description: "定位页面中的文字区域，为文字识别提供位置。",
      },
      {
        role: "文字识别",
        name: "PP-OCRv5_server_rec",
        downloadUrl:
          "https://huggingface.co/PaddlePaddle/PP-OCRv5_server_rec/tree/b26c3587fda8da3c8ec0ce357214b4d661ff1558",
        description: "将扫描页、图片中的文字转换为可检索的文本。",
      },
      {
        role: "版面分析",
        name: "PP-DocLayout-S",
        downloadUrl:
          "https://huggingface.co/PaddlePaddle/PP-DocLayout-S/tree/8ac289e66575bb9bba6e15c53719d8b15cc9b3b2",
        description: "识别正文、标题、图片和表格等区域，辅助整理阅读顺序。",
      },
    ],
  },
  {
    title: "资料检索",
    description: "用于查找与问题相关的教材内容。",
    models: [
      {
        role: "向量检索 · Embedding",
        name: "BAAI/bge-small-zh-v1.5",
        downloadUrl:
          "https://huggingface.co/BAAI/bge-small-zh-v1.5/tree/7999e1d3359715c523056ef9478215996d62a620",
        description: "将教材片段和查询转换为向量，用于语义检索与混合检索。",
      },
      {
        role: "原文重排 · Reranker",
        name: "BAAI/bge-reranker-base",
        downloadUrl:
          "https://huggingface.co/BAAI/bge-reranker-base/tree/2cfc18c9415c912f9d8155881c133215df768a70",
        description: "对候选原文重新排序，在“相关原文”中展示最多 5 条结果。",
      },
    ],
  },
];

export function PdfModelOverview() {
  const [linkError, setLinkError] = useState<string | null>(null);

  return (
    <section className="pdf-model-overview" aria-labelledby="pdf-models-title">
      <div className="pdf-model-heading">
        <h2 id="pdf-models-title">PDF 知识库模型</h2>
        <span className="pdf-model-badge">本地运行 · 无需 API Key</span>
      </div>
      <p className="pdf-model-intro">
        PDF
        知识库使用以下固定模型。模型文件准备完成后，识别和检索可在本机离线运行。
        下载页对应当前使用的模型版本。
      </p>
      {linkError ? (
        <p className="inline-error" role="alert">
          {linkError}
        </p>
      ) : null}
      {modelGroups.map((group) => (
        <section className="pdf-model-group" key={group.title}>
          <h3>{group.title}</h3>
          <p className="pdf-model-group-note">{group.description}</p>
          <ul className="pdf-model-list">
            {group.models.map((model) => (
              <li key={model.name}>
                <span className="pdf-model-role">{model.role}</span>
                <strong className="pdf-model-name">{model.name}</strong>
                <p>{model.description}</p>
                <a
                  className="pdf-model-download"
                  href={model.downloadUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  aria-label={`打开 ${model.name} 的 Hugging Face 下载页（新窗口）`}
                  onClick={(event) => {
                    if (!isDesktop()) return;
                    event.preventDefault();
                    setLinkError(null);
                    void openSourceUrl(model.downloadUrl).catch(
                      (error: unknown) =>
                        setLinkError(`${model.name}：${friendlyError(error)}`),
                    );
                  }}
                >
                  Hugging Face 下载页 <span aria-hidden="true">↗</span>
                </a>
              </li>
            ))}
          </ul>
        </section>
      ))}
      <p className="model-call-note">
        PDF 中的知识卡生成、AI 解释和复习 Agent 使用已配置的 DeepSeek 或 Kimi
        在线模型，需要联网。可在“在线生成模型”中配置对应通道。
      </p>
    </section>
  );
}
