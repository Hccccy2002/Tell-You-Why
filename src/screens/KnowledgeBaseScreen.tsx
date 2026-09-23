import { useEffect, useState } from "react";
import { LearningPanel } from "../components/LearningPanel";
import { ReviewAgentPanel } from "../components/ReviewAgentPanel";
import { ConfirmationDialog } from "../components/ConfirmationDialog";
import "../knowledge-base.css";
import { friendlyError } from "../lib/api";
import { useAutoHideGuard } from "../lib/autoHideGuard";
import {
  choosePdf,
  deletePdf,
  importPdf,
  kbRead,
  pausePdf,
  resumePdf,
  type Catalog,
  type Chapter,
  type OcrMode,
  type PagePreview,
  type Passage,
  type PdfKnowledgeBase,
  type PdfSelection,
} from "../lib/knowledgeBase";

const stageNames: Record<string, string> = {
  verifying: "正在检查本地模型",
  extracting: "正在提取文字 / OCR",
  structure: "正在整理章节",
  chunks: "正在分块",
  indexing: "正在整理章节并建立索引",
  published: "处理完成",
};

function statusText(book: PdfKnowledgeBase) {
  if (book.job?.status === "running") return "正在处理";
  if (book.job?.status === "cancelling") return "正在暂停";
  if (book.job?.status === "failed") return "处理失败";
  if (book.version)
    return book.status === "partial_ready" ? "部分内容可用" : "可以浏览";
  return "等待继续";
}

export function KnowledgeBaseScreen() {
  useAutoHideGuard(true, "pdf-knowledge-base");
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [selection, setSelection] = useState<PdfSelection | null>(null);
  const [firstPage, setFirstPage] = useState(1);
  const [ocrMode, setOcrMode] = useState<OcrMode>("always");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<PdfKnowledgeBase | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);

  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const result = await kbRead<Catalog>({ op: "catalog" });
        if (active) {
          setCatalog(result);
          setPollError(null);
        }
      } catch (reason) {
        if (active) setPollError(friendlyError(reason));
      } finally {
        if (active) timer = setTimeout(() => void poll(), 3000);
      }
    }
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [refresh]);

  async function choose() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const pdf = await choosePdf();
      if (pdf) {
        setSelection(pdf);
        setFirstPage(1);
      }
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      setBusy(false);
    }
  }

  async function start() {
    if (!selection) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await importPdf(selection.path, firstPage, ocrMode);
      setSelection(null);
      setSelectedId(result.kb);
      setRefresh((v) => v + 1);
      setNotice(
        result.reused
          ? "已打开这份 PDF 的现有知识库，无需重复处理。"
          : "已开始后台处理。可以切换页面，稍后回来查看进度。",
      );
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      setBusy(false);
    }
  }

  async function control(book: PdfKnowledgeBase, action: "pause" | "resume") {
    if (!book.job) return;
    setBusy(true);
    setError(null);
    try {
      if (action === "pause") {
        await pausePdf(book.id, book.job.id);
        setNotice(
          "已请求暂停；当前页面会先保存。若已进入索引阶段，将完成本次索引。",
        );
      } else {
        await resumePdf(book.id, book.job.id);
        setNotice("已继续处理，已完成的页面会被复用。");
      }
      setRefresh((v) => v + 1);
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!pendingDelete) return;
    const target = pendingDelete;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await deletePdf(target.id);
      setCatalog((value) =>
        value
          ? {
              ...value,
              items: value.items.filter((item) => item.id !== target.id),
            }
          : value,
      );
      if (selectedId === target.id) setSelectedId(null);
      setNotice(`已删除“${target.filename}”及其本地处理数据。`);
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      setPendingDelete(null);
      setBusy(false);
    }
  }

  const selected = catalog?.items.find((book) => book.id === selectedId);
  const importDisabled = busy || !catalog || catalog.import_running;

  return (
    <main className="page-view kb-view">
      <div className="page-heading">
        <span className="eyebrow">从自己的资料开始</span>
        <h1>PDF 知识库</h1>
        <p className="kb-muted">导入教材或文档，按章节阅读、检索并核对原文。</p>
      </div>
      {error || pollError ? (
        <div className="kb-error" role="alert">
          {error || pollError}
          <button
            className="text-button"
            onClick={() => {
              setError(null);
              setRefresh((v) => v + 1);
            }}
          >
            重试连接
          </button>
        </div>
      ) : null}
      {notice ? (
        <p className="kb-notice" role="status">
          {notice}
        </p>
      ) : null}
      {catalog?.launch_error ? (
        <p className="kb-error" role="alert">
          {catalog.launch_error}
        </p>
      ) : null}
      {catalog?.errors.map((item) => (
        <p className="kb-error" key={item.id}>
          知识库 {item.id} 无法读取：{item.error}
        </p>
      ))}

      {selected ? (
        <>
          <button
            className="text-button kb-back"
            onClick={() => {
              setSelectedId(null);
              setNotice(null);
            }}
          >
            ← 全部知识库
          </button>
          <section className="kb-panel" aria-label="知识库概况">
            <h2 className="kb-filename">{selected.filename}</h2>
            <div className="kb-meta">
              <span className="kb-badge">{statusText(selected)}</span>
              <span>
                {selected.pages} 页 · {selected.chunks.toLocaleString()} 个片段
              </span>
            </div>
            {selected.job && selected.job.status !== "completed" ? (
              <div className="kb-progress">
                <p role="status">
                  {selected.job.status === "running"
                    ? stageNames[selected.job.stage] || "正在处理"
                    : statusText(selected)}{" "}
                  · 已处理 {selected.job.completed} / {selected.job.total} 页
                </p>
                <progress
                  max={selected.job.total}
                  value={selected.job.completed}
                  aria-label="PDF 页面处理进度"
                />
                <p className="kb-muted">
                  {selected.job.counts.needs_review || 0} 页含待核对区域 ·{" "}
                  {selected.job.counts.failed || 0} 页失败
                </p>
                {selected.job.error ? (
                  <p role="alert" className="kb-error">
                    {selected.job.error}
                  </p>
                ) : null}
                {selected.job.page_errors.map((item) => (
                  <p className="kb-muted" key={item.page}>
                    第 {item.page} 页：{item.error}
                  </p>
                ))}
                {selected.job.status === "running" ||
                selected.job.status === "cancelling" ? (
                  <button
                    className="secondary-button"
                    disabled={busy || selected.job.status === "cancelling"}
                    onClick={() => void control(selected, "pause")}
                  >
                    {selected.job.status === "cancelling"
                      ? "正在暂停…"
                      : "暂停处理"}
                  </button>
                ) : (
                  <button
                    className="primary-button"
                    disabled={busy || catalog?.import_running}
                    onClick={() => void control(selected, "resume")}
                  >
                    继续处理
                  </button>
                )}
              </div>
            ) : null}
            {selected.status === "partial_ready" ? (
              <p className="kb-quality">
                可检索已通过检查的文字。部分图表、公式和识别不确定的内容未纳入检索，请以原文为准。
                {selected.coverage
                  ? ` 已识别正文中约 ${(selected.coverage.eligible_fraction_of_recognized_body * 100).toFixed(1)}% 的文字可检索。`
                  : ""}
              </p>
            ) : null}
            <button
              className="kb-delete-button kb-detail-delete"
              disabled={busy}
              onClick={() => setPendingDelete(selected)}
            >
              删除这份 PDF
            </button>
          </section>
          {selected.version ? (
            <BookContent key={selected.id + selected.version} book={selected} />
          ) : (
            <div className="kb-empty">
              <h2>正在准备这份资料</h2>
              <p>完成识别、章节整理和索引后，即可在这里浏览和检索。</p>
            </div>
          )}
        </>
      ) : (
        <>
          <section className="kb-panel kb-import" aria-label="导入 PDF">
            <div className="kb-import-heading">
              <span className="kb-document-icon" aria-hidden="true">
                PDF
              </span>
              <div>
                <h2>把书放进知识库</h2>
                <p className="kb-muted">支持扫描版 PDF · 在本机处理与保存</p>
              </div>
            </div>
            {selection ? (
              <div className="kb-selected-file">
                <strong className="kb-filename">{selection.filename}</strong>
                <p className="kb-muted">
                  {selection.pages} 页 ·{" "}
                  {(selection.bytes / 1024 / 1024).toFixed(1)} MB
                </p>
                <label className="kb-field">
                  正文起始页
                  <input
                    type="number"
                    min={1}
                    max={selection.pages}
                    value={firstPage}
                    onChange={(e) => setFirstPage(Number(e.target.value))}
                    disabled={busy}
                  />
                </label>
                <p className="kb-muted">
                  使用 PDF
                  的实际页序号。此前页面仍会保留原文，但不纳入知识片段。
                </p>
                <fieldset className="kb-ocr-field" disabled={busy}>
                  <legend>文字识别模式</legend>
                  <div className="kb-ocr-options">
                    <label
                      className={
                        ocrMode === "always"
                          ? "kb-ocr-option selected"
                          : "kb-ocr-option"
                      }
                    >
                      <input
                        type="radio"
                        name="ocr-mode"
                        value="always"
                        checked={ocrMode === "always"}
                        aria-describedby="kb-ocr-always-tip"
                        onChange={() => setOcrMode("always")}
                      />
                      <span>
                        <strong>始终 OCR</strong>
                        <small>always · 扫描件优先</small>
                      </span>
                      <span
                        id="kb-ocr-always-tip"
                        role="tooltip"
                        className="kb-ocr-tooltip"
                      >
                        每页都渲染并进行 OCR。适合扫描版或文字层不可靠的
                        PDF；速度较慢，电子版文字也会重新识别。
                      </span>
                    </label>
                    <label
                      className={
                        ocrMode === "auto"
                          ? "kb-ocr-option selected"
                          : "kb-ocr-option"
                      }
                    >
                      <input
                        type="radio"
                        name="ocr-mode"
                        value="auto"
                        checked={ocrMode === "auto"}
                        aria-describedby="kb-ocr-auto-tip"
                        onChange={() => setOcrMode("auto")}
                      />
                      <span>
                        <strong>智能选择</strong>
                        <small>auto · 电子版更快</small>
                      </span>
                      <span
                        id="kb-ocr-auto-tip"
                        role="tooltip"
                        className="kb-ocr-tooltip"
                      >
                        优先使用 PDF 自带的可用文字层，无法使用的页面再执行
                        OCR。适合电子版或混合 PDF，通常更快。
                      </span>
                    </label>
                  </div>
                </fieldset>
                <div className="kb-actions">
                  <button
                    className="primary-button"
                    onClick={() => void start()}
                    disabled={
                      importDisabled ||
                      !Number.isInteger(firstPage) ||
                      firstPage < 1 ||
                      firstPage > selection.pages
                    }
                  >
                    {busy ? "正在准备…" : "开始导入"}
                  </button>
                  <button
                    className="secondary-button"
                    onClick={() => setSelection(null)}
                    disabled={busy}
                  >
                    取消选择
                  </button>
                </div>
              </div>
            ) : (
              <button
                className="primary-button"
                onClick={() => void choose()}
                disabled={importDisabled}
              >
                {busy ? "正在读取 PDF…" : "选择 PDF"}
              </button>
            )}
            <p className="kb-muted">
              大部头教材需要较长时间，可查看进度或暂停后继续。
            </p>
            {catalog?.import_running ? (
              <p className="kb-notice">
                已有资料正在处理，请在下方打开查看进度。
              </p>
            ) : null}
            {catalog && !catalog.models_ready ? (
              <p className="kb-error">
                本机尚未安装识别模型，请按项目 README 完成模型准备。
              </p>
            ) : null}
          </section>
          <div className="kb-list-heading">
            <h2>我的资料{catalog ? ` · ${catalog.items.length}` : ""}</h2>
            <button
              className="text-button"
              onClick={() => setRefresh((v) => v + 1)}
            >
              刷新
            </button>
          </div>
          {!catalog && !pollError ? <p role="status">正在读取知识库…</p> : null}
          {catalog?.items.length === 0 ? (
            <div className="kb-empty">
              <h2>第一份资料，从一本书开始</h2>
              <p>点击上方“选择 PDF”，完成后即可在这里找到它。</p>
            </div>
          ) : null}
          {catalog?.items.map((book) => (
            <div className="kb-book-row" key={book.id}>
              <button
                className="kb-book"
                onClick={() => {
                  setSelectedId(book.id);
                  setNotice(null);
                }}
              >
                <span className="kb-book-icon" aria-hidden="true">
                  ▤
                </span>
                <span className="kb-book-copy">
                  <strong className="kb-filename" id={`kb-book-${book.id}`}>
                    {book.filename}
                  </strong>
                  <span>
                    {book.pages} 页 · {book.chunks.toLocaleString()} 个片段
                  </span>
                  <span className="kb-book-status">
                    {statusText(book)}
                    {book.job && book.job.status !== "completed"
                      ? ` · ${book.job.completed}/${book.job.total} 页`
                      : ""}
                  </span>
                </span>
                <span aria-hidden="true">›</span>
              </button>
              <button
                className="kb-delete-button kb-book-delete"
                aria-label="删除 PDF"
                aria-describedby={`kb-book-${book.id}`}
                title={`删除 ${book.filename}`}
                disabled={busy}
                onClick={() => setPendingDelete(book)}
              >
                删除
              </button>
            </div>
          ))}
        </>
      )}
      {pendingDelete ? (
        <ConfirmationDialog
          id="delete-pdf"
          title={`删除“${pendingDelete.filename}”？`}
          confirmLabel="删除 PDF"
          busyLabel="正在停止并删除…"
          busy={busy}
          onCancel={() => setPendingDelete(null)}
          onConfirm={() => void remove()}
        >
          <p>原 PDF 副本、识别结果和检索索引将从本机永久删除。</p>
          {pendingDelete.job?.status === "running" ||
          pendingDelete.job?.status === "cancelling" ? (
            <p>当前导入任务会先安全停止；正在处理的页面保存完毕后再删除。</p>
          ) : null}
        </ConfirmationDialog>
      ) : null}
    </main>
  );
}

function BookContent({ book }: { book: PdfKnowledgeBase }) {
  const [tab, setTab] = useState<"search" | "page" | "learning" | "review">(
    "search",
  );
  const [learningOpened, setLearningOpened] = useState(false);
  const [reviewOpened, setReviewOpened] = useState(false);
  const [pageVersion, setPageVersion] = useState<string | undefined>();
  const [chapters, setChapters] = useState<Chapter[]>([]);
  const [chapter, setChapter] = useState("");
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState("hybrid");
  const [searchRequest, setSearchRequest] = useState<{
    query: string;
    mode: string;
    chapter: string;
    attempt: number;
  } | null>(null);
  const [results, setResults] = useState<Passage[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [page, setPage] = useState(1);
  const [pageInput, setPageInput] = useState("1");
  const [preview, setPreview] = useState<PagePreview | null>(null);
  const [zoom, setZoom] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    let active = true;
    void kbRead<Chapter[]>({ op: "chapters", kb: book.id })
      .then((value) => {
        if (active) setChapters(value);
      })
      .catch((reason) => {
        if (active) setError(friendlyError(reason));
      });
    return () => {
      active = false;
    };
  }, [book.id, reload]);

  useEffect(() => {
    if (!searchRequest) return;
    let active = true;
    void kbRead<{ results: Passage[] }>({
      op: "search",
      kb: book.id,
      query: searchRequest.query,
      mode: searchRequest.mode,
      chapter: searchRequest.chapter || null,
    })
      .then((value) => {
        if (active) setResults(value.results);
      })
      .catch((reason) => {
        if (active) setError(friendlyError(reason));
      })
      .finally(() => {
        if (active) setSearching(false);
      });
    return () => {
      active = false;
    };
  }, [book.id, searchRequest]);

  useEffect(() => {
    if (tab !== "page") return;
    let active = true;
    void kbRead<PagePreview>({
      op: "page",
      kb: book.id,
      page,
      ...(pageVersion ? { version: pageVersion } : {}),
    })
      .then((value) => {
        if (active) setPreview(value);
      })
      .catch((reason) => {
        if (active) setError(friendlyError(reason));
      });
    return () => {
      active = false;
    };
  }, [book.id, tab, page, reload, pageVersion]);

  function goPage(value: number, version?: string) {
    if (!Number.isInteger(value) || value < 1 || value > book.pages) {
      setError(`请输入 1–${book.pages} 之间的页码。`);
      return;
    }
    setPage(value);
    setPageVersion(version);
    setPageInput(String(value));
    setPreview(null);
    setError(null);
    setTab("page");
    setReload((v) => v + 1);
  }

  function changeChapter(value: string) {
    setChapter(value);
    setResults(null);
    setSearchRequest(null);
    setSearching(false);
    setError(null);
  }

  return (
    <section className="kb-content" aria-label="资料内容">
      <div className="segmented-control" aria-label="资料查看方式">
        {(
          [
            ["search", "检索"],
            ["learning", "随机学习"],
            ["review", "复习 Agent"],
            ["page", "原文"],
          ] as const
        ).map(([value, label]) => (
          <button
            key={value}
            aria-pressed={tab === value}
            className={tab === value ? "active" : ""}
            onClick={() => {
              setTab(value);
              if (value === "learning") setLearningOpened(true);
              if (value === "review") setReviewOpened(true);
              setError(null);
            }}
          >
            {label}
          </button>
        ))}
      </div>
      {error ? (
        <div className="kb-error" role="alert">
          {error}
          <button
            className="text-button"
            onClick={() => {
              setError(null);
              setReload((v) => v + 1);
              if (tab === "search" && searchRequest) {
                setResults(null);
                setSearching(true);
                setSearchRequest({
                  ...searchRequest,
                  attempt: searchRequest.attempt + 1,
                });
              }
            }}
          >
            重新加载
          </button>
        </div>
      ) : null}
      {tab === "search" ? (
        <label className="kb-field">
          章节范围
          <select
            value={chapter}
            onChange={(e) => changeChapter(e.target.value)}
          >
            <option value="">全部章节</option>
            {chapters.map((item) => (
              <option value={item.id} key={item.id}>
                {item.title}
              </option>
            ))}
          </select>
        </label>
      ) : null}
      {tab === "search" ? (
        <>
          <form
            className="kb-search"
            onSubmit={(e) => {
              e.preventDefault();
              if (!query.trim() || searching) return;
              setError(null);
              setResults(null);
              setSearching(true);
              setSearchRequest({
                query: query.trim(),
                mode,
                chapter,
                attempt: (searchRequest?.attempt || 0) + 1,
              });
            }}
          >
            <label className="kb-field">
              检索内容
              <textarea
                value={query}
                maxLength={1000}
                rows={2}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="例如：总线的作用是什么？"
              />
            </label>
            <div className="kb-search-actions">
              <label>
                <span className="sr-only">检索方式</span>
                <select value={mode} onChange={(e) => setMode(e.target.value)}>
                  <option value="hybrid">语义 + 关键词</option>
                  <option value="keyword">仅关键词</option>
                </select>
              </label>
              <button
                className="primary-button"
                disabled={!query.trim() || searching}
              >
                {searching ? "正在检索…" : "检索原文"}
              </button>
            </div>
          </form>
          <p className="kb-muted">
            这里返回相关原文。首次语义检索需要加载本地模型。
          </p>
          {searching ? <p role="status">正在寻找相关原文…</p> : null}
          {results ? (
            <>
              <p className="kb-muted">
                “{searchRequest?.query}” · 找到 {results.length} 个相关片段
              </p>
              <PassageList items={results} onPage={goPage} />
              {results.length === 0 ? (
                <p className="kb-empty">
                  未找到匹配片段。可以换个说法、扩大章节范围，或查看原文。
                </p>
              ) : null}
            </>
          ) : null}
        </>
      ) : null}
      {learningOpened ? (
        <div hidden={tab !== "learning"}>
          <LearningPanel
            book={book}
            chapter={chapter}
            chapters={chapters}
            active={tab === "learning"}
            onPage={goPage}
            onRestoreChapter={setChapter}
            onChapterChange={changeChapter}
          />
        </div>
      ) : null}
      {reviewOpened && (
        <div hidden={tab !== "review"}>
          <ReviewAgentPanel
            book={book}
            chapters={chapters}
            active={tab === "review"}
            onPage={goPage}
          />
        </div>
      )}
      {tab === "page" ? (
        <>
          <form
            className="kb-page-controls"
            onSubmit={(e) => {
              e.preventDefault();
              goPage(Number(pageInput), pageVersion);
            }}
          >
            <button
              type="button"
              className="secondary-button"
              disabled={page <= 1}
              onClick={() => goPage(page - 1, pageVersion)}
              aria-label="上一页原文"
            >
              ‹
            </button>
            <label>
              第{" "}
              <input
                aria-label="原文页码"
                type="number"
                min={1}
                max={book.pages}
                value={pageInput}
                onChange={(e) => setPageInput(e.target.value)}
              />{" "}
              / {book.pages} 页
            </label>
            <button className="secondary-button">跳转</button>
            <button
              type="button"
              className="secondary-button"
              disabled={page >= book.pages}
              onClick={() => goPage(page + 1, pageVersion)}
              aria-label="下一页原文"
            >
              ›
            </button>
          </form>
          <div className="kb-list-heading">
            <p className="kb-muted">
              PDF 实际页序号
              {preview?.printed_label
                ? ` · 书内页码 ${preview.printed_label}`
                : ""}
            </p>
            <button className="text-button" onClick={() => setZoom((v) => !v)}>
              {zoom ? "适应宽度" : "放大查看"}
            </button>
          </div>
          {preview ? (
            <div className={zoom ? "kb-page-image zoomed" : "kb-page-image"}>
              <img
                src={preview.image}
                alt={`${book.filename} 第 ${preview.page} 页原文`}
              />
            </div>
          ) : !error ? (
            <p role="status">正在读取原文页面…</p>
          ) : null}
        </>
      ) : null}
    </section>
  );
}

function PassageList({
  items,
  onPage,
}: {
  items: Passage[];
  onPage: (page: number) => void;
}) {
  return (
    <div className="kb-passages">
      {items.map((item, i) => (
        <article className="kb-passage" key={item.chunk_id || item.id || i}>
          <p className="kb-passage-chapter">
            {item.chapter_path.join(" / ") || "正文"}
          </p>
          <p className="kb-passage-text">{item.text}</p>
          <div className="kb-citations">
            {Array.from(
              new Set(item.locations.map((location) => location.page)),
            ).map((page) => (
              <button
                key={page}
                className="text-button"
                onClick={() => onPage(page)}
              >
                查看第 {page} 页原文 ↗
              </button>
            ))}
          </div>
        </article>
      ))}
    </div>
  );
}
