import { useMemo, useRef, useState, type PointerEvent } from "react";
import { ConfirmationDialog } from "../components/ConfirmationDialog";
import { friendlyError, saveInterests } from "../lib/api";
import type { TopicPreference } from "../types";

interface Props {
  initialTopics: TopicPreference[];
  personalizationEnabled: boolean;
  onSaved: (topics: TopicPreference[], personalization: boolean) => void;
}

const riskyWords = [
  "医疗诊断",
  "法律建议",
  "投资建议",
  "实时政治",
  "博彩",
  "成人内容",
];

export function InterestSettingsScreen({
  initialTopics,
  personalizationEnabled,
  onSaved,
}: Props) {
  const [topics, setTopics] = useState(() =>
    initialTopics.map((topic) => ({ ...topic })),
  );
  const [personalization, setPersonalization] = useState(
    personalizationEnabled,
  );
  const [customText, setCustomText] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [draggedId, setDraggedId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const [dropAfter, setDropAfter] = useState(false);
  const [topicToDelete, setTopicToDelete] = useState<{
    id: string;
    label: string;
  } | null>(null);
  const pointerDrag = useRef<{
    pointerId: number;
    sourceId: string;
    startY: number;
    active: boolean;
  } | null>(null);
  const suppressClick = useRef(false);

  const selectedCount = topics.filter(
    (topic) => topic.selected && topic.enabled,
  ).length;
  const generationWeights = useMemo(() => {
    return new Map(
      topics.map((topic, index) => [topic.id, topics.length - index]),
    );
  }, [topics]);

  function update(id: string, patch: Partial<TopicPreference>) {
    setTopics((items) =>
      items.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    );
    setMessage(null);
  }

  function applyOrder(items: TopicPreference[]) {
    return items.map((topic, rank) => ({ ...topic, rank }));
  }

  function reorder(sourceId: string, targetId: string, insertAfter: boolean) {
    if (sourceId === targetId) return;
    setTopics((items) => {
      const sourceIndex = items.findIndex((topic) => topic.id === sourceId);
      if (sourceIndex < 0) return items;
      const next = [...items];
      const [moved] = next.splice(sourceIndex, 1);
      let targetIndex = next.findIndex((topic) => topic.id === targetId);
      if (!moved || targetIndex < 0) return items;
      if (insertAfter) targetIndex += 1;
      next.splice(targetIndex, 0, moved);
      return applyOrder(next);
    });
    setMessage(null);
  }

  function pinToTop(id: string) {
    setTopics((items) => {
      const sourceIndex = items.findIndex((topic) => topic.id === id);
      if (sourceIndex <= 0) return items;
      const next = [...items];
      const [moved] = next.splice(sourceIndex, 1);
      return moved ? applyOrder([moved, ...next]) : items;
    });
    setMessage(null);
  }

  function beginPointerDrag(
    event: PointerEvent<HTMLDivElement>,
    sourceId: string,
  ) {
    if (busy || event.button !== 0) return;
    const target = event.target as Element;
    if (target.closest("button, input, select")) return;
    pointerDrag.current = {
      pointerId: event.pointerId,
      sourceId,
      startY: event.clientY,
      active: false,
    };
  }

  function movePointerDrag(event: PointerEvent<HTMLDivElement>) {
    const drag = pointerDrag.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (!drag.active) {
      if (Math.abs(event.clientY - drag.startY) < 4) return;
      drag.active = true;
      setDraggedId(drag.sourceId);
      event.currentTarget.setPointerCapture?.(event.pointerId);
    }
    event.preventDefault();
    const targetRow = document
      .elementFromPoint(event.clientX, event.clientY)
      ?.closest<HTMLElement>("[data-topic-id]");
    const targetId = targetRow?.dataset.topicId;
    if (!targetId || targetId === drag.sourceId) return;
    const rect = targetRow.getBoundingClientRect();
    const insertAfter = event.clientY >= rect.top + rect.height / 2;
    setDragOverId(targetId);
    setDropAfter(insertAfter);
    reorder(drag.sourceId, targetId, insertAfter);
  }

  function finishPointerDrag(event: PointerEvent<HTMLDivElement>) {
    const drag = pointerDrag.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (drag.active) {
      suppressClick.current = true;
      window.setTimeout(() => {
        suppressClick.current = false;
      }, 0);
      if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
    }
    pointerDrag.current = null;
    setDraggedId(null);
    setDragOverId(null);
    setDropAfter(false);
  }

  function addCustom() {
    const label = customText.trim();
    if (!label) return;
    if ([...label].length > 30) {
      setMessage("单个自定义兴趣不能超过 30 个中文字符。");
      return;
    }
    if (riskyWords.some((word) => label.includes(word))) {
      setMessage("这个主题不在 MVP 的安全内容范围内。");
      return;
    }
    if (topics.some((topic) => topic.label === label)) {
      setMessage("这个兴趣已经存在。");
      return;
    }
    setTopics((items) =>
      applyOrder([
        ...items,
        {
          id: `custom-${crypto.randomUUID()}`,
          label,
          selected: true,
          enabled: true,
          custom: true,
          rank: items.length,
          weight: 0,
        },
      ]),
    );
    setCustomText("");
  }

  async function save() {
    if (selectedCount < 3) {
      setMessage("请至少保留 3 个已启用兴趣。");
      return;
    }
    setBusy(true);
    try {
      const saved = await saveInterests(topics, personalization);
      setTopics(saved);
      onSaved(saved, personalization);
      setMessage("兴趣设置已保存在本机。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="page-view interest-settings">
      <div className="page-heading">
        <span className="eyebrow">排序就是生成偏好</span>
        <h1>兴趣设置</h1>
        <p>拖动兴趣排序；越靠上，随机生成该领域知识点的概率越高。</p>
      </div>
      <label className="switch-row">
        <span>
          <strong>启用个性化排序</strong>
          <small>阅读反馈只调整本地内容推荐</small>
        </span>
        <input
          type="checkbox"
          checked={personalization}
          onChange={(event) => setPersonalization(event.target.checked)}
        />
      </label>
      <div className="settings-section-heading">
        <h2>兴趣列表</h2>
        <span>{selectedCount} 个参与随机生成</span>
      </div>
      <p className="reorder-hint">
        用鼠标按住任一兴趣框并上下拖动；列表会实时排序并更新权重。
      </p>
      <div className="reorder-list" role="list" aria-label="兴趣权重排序">
        {topics.map((topic, index) => {
          const generationWeight = generationWeights.get(topic.id);
          const classes = [
            "interest-row",
            !topic.enabled ? "muted" : "",
            draggedId === topic.id ? "dragging" : "",
            dragOverId === topic.id ? "drag-over" : "",
            dragOverId === topic.id && dropAfter ? "drag-over-after" : "",
          ]
            .filter(Boolean)
            .join(" ");
          return (
            <div
              key={topic.id}
              className={classes}
              role="listitem"
              data-topic-id={topic.id}
              onPointerDown={(event) => beginPointerDrag(event, topic.id)}
              onPointerMove={movePointerDrag}
              onPointerUp={finishPointerDrag}
              onPointerCancel={finishPointerDrag}
              onClickCapture={(event) => {
                if (!suppressClick.current) return;
                event.preventDefault();
                event.stopPropagation();
              }}
            >
              <span className="drag-handle" aria-hidden="true">
                <span aria-hidden="true">⠿</span>
              </span>
              <label>
                <input
                  type="checkbox"
                  checked={topic.selected && topic.enabled}
                  disabled={!topic.enabled}
                  onChange={(event) =>
                    update(topic.id, { selected: event.target.checked })
                  }
                />
                <span>
                  <strong>{topic.label}</strong>
                  <small>
                    {topic.custom ? "自定义 · " : ""}
                    {topic.selected && topic.enabled
                      ? `生成权重 ${generationWeight}`
                      : `未参与随机生成 · 排序权重 ${generationWeight}`}
                  </small>
                </span>
              </label>
              <div className="row-actions">
                <button
                  aria-label={`置顶 ${topic.label}`}
                  disabled={busy || index === 0}
                  onClick={() => pinToTop(topic.id)}
                >
                  置顶
                </button>
                {topic.custom ? (
                  <button
                    aria-label={`删除 ${topic.label}`}
                    disabled={busy}
                    onClick={() =>
                      setTopicToDelete({ id: topic.id, label: topic.label })
                    }
                  >
                    删除
                  </button>
                ) : (
                  <button
                    disabled={busy}
                    onClick={() =>
                      update(topic.id, {
                        enabled: !topic.enabled,
                        selected: topic.enabled ? false : topic.selected,
                      })
                    }
                  >
                    {topic.enabled ? "停用" : "启用"}
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
      <div className="custom-interest-row settings-custom">
        <label htmlFor="settings-custom-interest">添加自定义兴趣</label>
        <div>
          <input
            id="settings-custom-interest"
            value={customText}
            maxLength={30}
            placeholder="例如：城市规划"
            onChange={(event) => setCustomText(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                addCustom();
              }
            }}
          />
          <button className="secondary-button" onClick={addCustom}>
            添加
          </button>
        </div>
      </div>
      {message ? (
        <p className="form-message" role="status">
          {message}
        </p>
      ) : null}
      <button
        className="primary-button wide page-save"
        disabled={busy}
        onClick={() => void save()}
      >
        {busy ? "正在保存…" : "保存兴趣设置"}
      </button>
      {topicToDelete ? (
        <ConfirmationDialog
          id="custom-interest-delete-confirmation"
          eyebrow="兴趣删除确认"
          title={`确认删除“${topicToDelete.label}”吗？`}
          confirmLabel="确认删除兴趣"
          busyLabel="正在删除…"
          busy={busy}
          onCancel={() => setTopicToDelete(null)}
          onConfirm={() => {
            setTopics((items) =>
              applyOrder(items.filter((item) => item.id !== topicToDelete.id)),
            );
            setMessage(null);
            setTopicToDelete(null);
          }}
        >
          <p>
            该自定义兴趣会从当前列表移除；只有点击“保存兴趣设置”后，删除才会写入本机。
          </p>
        </ConfirmationDialog>
      ) : null}
    </main>
  );
}
