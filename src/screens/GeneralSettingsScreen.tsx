import { useEffect, useState } from "react";
import {
  chooseAndImportCards,
  clearData,
  exitApplication,
  friendlyError,
  generationUsage,
  pauseReminders,
  saveSettings,
} from "../lib/api";
import { ConfirmationDialog } from "../components/ConfirmationDialog";
import type { AppSettings, DataClearScope } from "../types";
import type { ReminderPreset } from "../types";

interface Props {
  initialSettings: AppSettings;
  onSaved: (settings: AppSettings) => void;
  onDataCleared: (
    scope: DataClearScope,
    warning: string | null,
  ) => void | Promise<void>;
}

const dataClearConfirmations: Record<
  DataClearScope,
  {
    title: string;
    description: string;
    confirmLabel: string;
    busyLabel: string;
    successMessage: string;
  }
> = {
  history: {
    title: "清除所有阅读记录？",
    description:
      "将删除阅读、揭晓和展开记录；收藏、知识卡内容和模型配置会保留。",
    confirmLabel: "清除阅读记录",
    busyLabel: "正在清除阅读记录…",
    successMessage: "阅读记录已清除，收藏已保留。",
  },
  preferences: {
    title: "清除推荐偏好？",
    description:
      "将清零兴趣权重并删除所有自定义兴趣；内置兴趣的选择、启停和排序会保留。阅读记录、收藏和模型配置不受影响。",
    confirmLabel: "清除偏好",
    busyLabel: "正在清除偏好…",
    successMessage: "推荐偏好已重置。",
  },
  all: {
    title: "清除全部本地数据？",
    description:
      "将删除阅读记录、收藏、偏好、应用内设置、生成或导入的知识卡及 API Key，并回到首次使用状态；内置示例内容会保留。此操作无法撤销。",
    confirmLabel: "清除全部本地数据",
    busyLabel: "正在清除全部本地数据…",
    successMessage: "全部本地数据已清除。",
  },
};

export function GeneralSettingsScreen({
  initialSettings,
  onSaved,
  onDataCleared,
}: Props) {
  const [settings, setSettings] = useState(() => ({ ...initialSettings }));
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [generatedToday, setGeneratedToday] = useState(0);
  const [clearConfirmation, setClearConfirmation] =
    useState<DataClearScope | null>(null);

  useEffect(() => {
    let active = true;
    void generationUsage()
      .then((usage) => {
        if (active) setGeneratedToday(usage.generated);
      })
      .catch((error: unknown) => {
        if (active) setMessage(friendlyError(error));
      });
    return () => {
      active = false;
    };
  }, []);

  function update<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    setSettings((current) => ({ ...current, [key]: value }));
    setMessage(null);
  }

  function updateReminderTime(index: number, value: string) {
    const reminderTimes = [...settings.reminderTimes];
    reminderTimes[index] = value;
    update("reminderTimes", reminderTimes.filter(Boolean));
  }

  function toggleWeekday(day: number) {
    const weekdays = settings.weekdays.includes(day)
      ? settings.weekdays.filter((item) => item !== day)
      : [...settings.weekdays, day].sort();
    update("weekdays", weekdays);
  }

  function changeReminderPreset(reminderPreset: ReminderPreset) {
    setSettings((current) => {
      const reminderTimes =
        reminderPreset === "weekday_once"
          ? [current.reminderTimes[0] ?? "10:30"]
          : reminderPreset === "manual"
            ? current.reminderTimes
            : [
                current.reminderTimes[0] ?? "10:30",
                current.reminderTimes[1] ?? "15:30",
              ];
      return { ...current, reminderPreset, reminderTimes };
    });
    setMessage(null);
  }

  async function save() {
    setBusy("save");
    try {
      const saved = await saveSettings(settings);
      setSettings(saved);
      onSaved(saved);
      setMessage("设置已保存在本机。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function pause(mode: "thirty_minutes" | "today") {
    setBusy("pause");
    try {
      const until = await pauseReminders(mode);
      update("pauseUntil", until);
      setMessage(
        mode === "thirty_minutes" ? "已暂停提醒 30 分钟。" : "今天不再提醒。",
      );
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function importCards() {
    setBusy("import");
    try {
      const result = await chooseAndImportCards();
      if (result) {
        const detail = result.errors.length
          ? ` ${result.errors.slice(0, 2).join("；")}`
          : "";
        setMessage(
          `导入 ${result.imported} 张，跳过重复 ${result.duplicates} 张，拒绝 ${result.rejected} 张。${detail}`,
        );
      }
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function clear(scope: DataClearScope) {
    setBusy(`clear-${scope}`);
    try {
      const warning = await clearData(scope);
      await onDataCleared(scope, warning);
      setClearConfirmation(null);
      setMessage(warning ?? dataClearConfirmations[scope].successMessage);
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  return (
    <main className="page-view general-settings">
      <div className="page-heading">
        <span className="eyebrow">安静、可控、只在需要时出现</span>
        <h1>通用设置</h1>
      </div>
      <section className="settings-group">
        <h2>窗口与启动</h2>
        <label className="switch-row">
          <span>
            <strong>始终置顶</strong>
            <small>默认关闭</small>
          </span>
          <input
            type="checkbox"
            checked={settings.alwaysOnTop}
            onChange={(event) => update("alwaysOnTop", event.target.checked)}
          />
        </label>
        <label className="switch-row">
          <span>
            <strong>鼠标移出后自动收起</strong>
            <small>
              离开窗口约 0.7 秒后收至系统托盘；弹窗或拖动时不会触发。
            </small>
          </span>
          <input
            type="checkbox"
            checked={settings.autoHideOnMouseLeave}
            onChange={(event) =>
              update("autoHideOnMouseLeave", event.target.checked)
            }
          />
        </label>
        <label className="switch-row">
          <span>
            <strong>开机启动</strong>
            <small>只启动到托盘</small>
          </span>
          <input
            type="checkbox"
            checked={settings.autostart}
            onChange={(event) => update("autostart", event.target.checked)}
          />
        </label>
        <label className="field-row">
          <span>主题</span>
          <select
            value={settings.theme}
            onChange={(event) =>
              update("theme", event.target.value as AppSettings["theme"])
            }
          >
            <option value="system">跟随系统</option>
            <option value="light">浅色</option>
            <option value="dark">深色</option>
          </select>
        </label>
        <label className="field-row stacked">
          <span>全局快捷键</span>
          <input
            value={settings.globalShortcut}
            onChange={(event) => update("globalShortcut", event.target.value)}
          />
          <small>默认 Alt+Shift+Y；冲突时保存会给出提示。</small>
        </label>
      </section>

      <section className="settings-group">
        <h2>提醒</h2>
        <label className="field-row">
          <span>频率</span>
          <select
            value={settings.reminderPreset}
            onChange={(event) =>
              changeReminderPreset(event.target.value as ReminderPreset)
            }
          >
            <option value="manual">仅手动打开</option>
            <option value="weekday_once">工作日每天 1 次</option>
            <option value="weekday_twice">工作日每天 2 次</option>
            <option value="custom">自定义时间</option>
          </select>
        </label>
        {settings.reminderPreset !== "manual" ? (
          <>
            <div className="two-fields">
              <label className="field-row stacked">
                <span>提醒时间</span>
                <input
                  type="time"
                  value={settings.reminderTimes[0] ?? "10:30"}
                  onChange={(event) =>
                    updateReminderTime(0, event.target.value)
                  }
                />
              </label>
              {settings.reminderPreset === "weekday_twice" ||
              settings.reminderPreset === "custom" ? (
                <label className="field-row stacked">
                  <span>第二次提醒</span>
                  <input
                    type="time"
                    value={settings.reminderTimes[1] ?? "15:30"}
                    onChange={(event) =>
                      updateReminderTime(1, event.target.value)
                    }
                  />
                </label>
              ) : (
                <div />
              )}
            </div>
            <fieldset className="weekday-picker">
              <legend>提醒日</legend>
              {[
                [1, "一"],
                [2, "二"],
                [3, "三"],
                [4, "四"],
                [5, "五"],
                [6, "六"],
                [7, "日"],
              ].map(([day, label]) => (
                <label key={day}>
                  <input
                    type="checkbox"
                    checked={settings.weekdays.includes(day as number)}
                    onChange={() => toggleWeekday(day as number)}
                  />
                  <span>{label}</span>
                </label>
              ))}
            </fieldset>
          </>
        ) : null}
        <div className="two-fields">
          <label className="field-row stacked">
            <span>静默开始</span>
            <input
              type="time"
              value={settings.quietStart}
              onChange={(event) => update("quietStart", event.target.value)}
            />
          </label>
          <label className="field-row stacked">
            <span>静默结束</span>
            <input
              type="time"
              value={settings.quietEnd}
              onChange={(event) => update("quietEnd", event.target.value)}
            />
          </label>
        </div>
        <div className="button-pair">
          <button
            className="secondary-button"
            disabled={busy != null}
            onClick={() => void pause("thirty_minutes")}
          >
            暂停 30 分钟
          </button>
          <button
            className="secondary-button"
            disabled={busy != null}
            onClick={() => void pause("today")}
          >
            今天暂停
          </button>
        </div>
      </section>

      <section className="settings-group">
        <h2>模型成本</h2>
        <label className="field-row stacked">
          <span>每日生成总数</span>
          <input
            aria-label="每日生成总数"
            type="number"
            min="1"
            max="1000"
            step="1"
            value={settings.dailyGenerationLimit}
            onWheel={(event) => {
              event.preventDefault();
              event.currentTarget.blur();
            }}
            onChange={(event) =>
              update(
                "dailyGenerationLimit",
                Math.trunc(Number(event.target.value)),
              )
            }
          />
          <strong className="generation-usage" aria-live="polite">
            已生成 {generatedToday}/{settings.dailyGenerationLimit}
          </strong>
          <small>可自定义 1–1000 张；只统计今天成功入库的知识点。</small>
        </label>
      </section>

      <section className="settings-group">
        <h2>内容与数据</h2>
        <button
          className="settings-action"
          disabled={busy != null}
          onClick={() => void importCards()}
        >
          <span>
            <strong>导入知识卡</strong>
            <small>支持经过字段校验的 JSON / CSV</small>
          </span>
          <span>{busy === "import" ? "导入中…" : "›"}</span>
        </button>
        <button
          className="settings-action"
          disabled={busy != null}
          onClick={() => {
            setMessage(null);
            setClearConfirmation("history");
          }}
        >
          <span>
            <strong>清除阅读记录</strong>
            <small>本机最多保留最近 500 条；清除不会删除收藏</small>
          </span>
          <span>›</span>
        </button>
        <button
          className="settings-action"
          disabled={busy != null}
          onClick={() => {
            setMessage(null);
            setClearConfirmation("preferences");
          }}
        >
          <span>
            <strong>清除偏好</strong>
            <small>重置兴趣权重和自定义兴趣</small>
          </span>
          <span>›</span>
        </button>
        <button
          className="settings-action danger"
          disabled={busy != null}
          onClick={() => {
            setMessage(null);
            setClearConfirmation("all");
          }}
        >
          <span>
            <strong>清除全部本地数据</strong>
            <small>包括模型系统凭据</small>
          </span>
          <span>›</span>
        </button>
      </section>
      {message && !clearConfirmation ? (
        <p className="form-message" role="status">
          {message}
        </p>
      ) : null}
      <button
        className="primary-button wide page-save"
        disabled={busy != null}
        onClick={() => void save()}
      >
        {busy === "save" ? "正在保存…" : "保存通用设置"}
      </button>
      <button className="exit-button" onClick={() => void exitApplication()}>
        退出 Tell You Why
      </button>
      <p className="privacy-footnote">
        兴趣、历史和反馈保存在本机。使用 AI
        功能时，会将相关问题、卡片及启用个性化时的相关学习记录发送给所选模型。不读取屏幕、浏览器记录、工作文件或剪贴板。
      </p>
      {clearConfirmation ? (
        <ConfirmationDialog
          id={`clear-${clearConfirmation}-confirmation`}
          eyebrow="清除确认"
          title={dataClearConfirmations[clearConfirmation].title}
          confirmLabel={dataClearConfirmations[clearConfirmation].confirmLabel}
          busyLabel={dataClearConfirmations[clearConfirmation].busyLabel}
          busy={busy === `clear-${clearConfirmation}`}
          onCancel={() => setClearConfirmation(null)}
          onConfirm={() => void clear(clearConfirmation)}
        >
          <p>{dataClearConfirmations[clearConfirmation].description}</p>
          {message ? (
            <p className="inline-error" role="alert">
              {message}
            </p>
          ) : null}
        </ConfirmationDialog>
      ) : null}
    </main>
  );
}
