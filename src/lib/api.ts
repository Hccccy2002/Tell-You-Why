import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { fallbackCards, presetTopics } from "../data/fallbackCards";
import { withAutoHideGuard } from "./autoHideGuard";
import type {
  AppSettings,
  BootstrapData,
  DataClearScope,
  FollowUpMessage,
  FollowUpResult,
  FollowUpTurn,
  GenerationBatchResult,
  GenerationUsage,
  ImportResult,
  InteractionKind,
  KnowledgeCard,
  LibraryItem,
  OnboardingInput,
  ProviderSpec,
  TopicPreference,
} from "../types";

const defaultSettings: AppSettings = {
  theme: "system",
  alwaysOnTop: false,
  autoHideOnMouseLeave: false,
  autostart: false,
  personalizationEnabled: true,
  reminderPreset: "manual",
  reminderTimes: ["11:00"],
  weekdays: [1, 2, 3, 4, 5],
  quietStart: "18:00",
  quietEnd: "09:00",
  pauseUntil: null,
  globalShortcut: "Alt+Shift+Y",
  dailyGenerationLimit: 10,
};

const fallbackProviders: ProviderSpec[] = [
  {
    id: "deepseek",
    label: "DeepSeek",
    regions: [{ id: "default", label: "官方服务" }],
    models: [
      {
        id: "deepseek-v4-flash",
        label: "DeepSeek V4 Flash",
        recommended: true,
      },
      { id: "deepseek-v4-pro", label: "DeepSeek V4 Pro", recommended: false },
    ],
    selectedRegion: "default",
    selectedModel: "deepseek-v4-flash",
    keyConfigured: false,
    keyLast4: null,
    connectionVerified: false,
  },
  {
    id: "kimi",
    label: "Kimi",
    regions: [
      { id: "cn", label: "中国大陆" },
      { id: "global", label: "国际服务" },
    ],
    models: [
      { id: "kimi-k3", label: "Kimi K3", recommended: true },
      { id: "kimi-k2.6", label: "Kimi K2.6", recommended: false },
    ],
    selectedRegion: "cn",
    selectedModel: "kimi-k3",
    keyConfigured: false,
    keyLast4: null,
    connectionVerified: false,
  },
];

const memory = {
  onboardingComplete: false,
  topics: presetTopics.map<TopicPreference>(([id, label], rank) => ({
    id,
    label,
    selected: false,
    enabled: true,
    custom: false,
    rank,
    weight: 0,
  })),
  settings: { ...defaultSettings },
  generationProviderId: "deepseek" as "deepseek" | "kimi",
  cursor: 0,
  history: [] as LibraryItem[],
  favorites: new Set<string>(),
  hidden: new Set<string>(),
  deleted: new Set<string>(),
  followUps: new Map<string, FollowUpMessage[]>(),
};

export const isDesktop = () =>
  typeof window !== "undefined" && window.__TAURI_INTERNALS__ != null;

function withFavorite(card: KnowledgeCard): KnowledgeCard {
  return {
    ...card,
    isFavorite: memory.favorites.has(card.id),
    hiddenFromFeed: memory.hidden.has(card.id),
  };
}

function addToHistory(card: KnowledgeCard) {
  memory.history = [
    { card: withFavorite(card), viewedAt: new Date().toISOString() },
    ...memory.history.filter((item) => item.card.id !== card.id),
  ].slice(0, 500);
}

function visibleFallbackCard(currentId?: string): KnowledgeCard | null {
  for (let offset = 1; offset <= fallbackCards.length; offset += 1) {
    const index = (memory.cursor + offset) % fallbackCards.length;
    const card = fallbackCards[index]!;
    if (
      card.id !== currentId &&
      !memory.hidden.has(card.id) &&
      !memory.deleted.has(card.id)
    ) {
      memory.cursor = index;
      return card;
    }
  }
  const remaining = fallbackCards.find(
    (card) => !memory.hidden.has(card.id) && !memory.deleted.has(card.id),
  );
  if (remaining) {
    memory.cursor = fallbackCards.indexOf(remaining);
    return remaining;
  }
  return null;
}

async function desktopOr<T>(
  command: string,
  args: Record<string, unknown>,
  fallback: () => T | Promise<T>,
) {
  if (isDesktop()) return invoke<T>(command, args);
  return fallback();
}

export async function bootstrapApp(options?: {
  recordShown?: boolean;
}): Promise<BootstrapData> {
  const fallback = (notice: string) => {
    const firstVisible = fallbackCards.find(
      (card) => !memory.hidden.has(card.id) && !memory.deleted.has(card.id),
    );
    const card = firstVisible ? withFavorite(firstVisible) : null;
    if (firstVisible && card && options?.recordShown !== false) {
      memory.cursor = fallbackCards.indexOf(firstVisible);
      addToHistory(card);
    }
    return {
      onboardingComplete: memory.onboardingComplete,
      card,
      availableCardCount: fallbackCards.filter(
        (item) => !memory.hidden.has(item.id) && !memory.deleted.has(item.id),
      ).length,
      topics: memory.topics.map((topic) => ({ ...topic })),
      settings: { ...memory.settings },
      providers: structuredClone(fallbackProviders),
      generationProviderId: memory.generationProviderId,
      desktopCapabilities: {
        tray: false,
        notifications: false,
        secureCredentials: false,
      },
      notice,
    };
  };
  if (!isDesktop()) {
    return fallback("当前为浏览器预览模式；桌面能力会在 Tauri 中启用。");
  }
  try {
    return await invoke<BootstrapData>("bootstrap_app", {
      recordShown: options?.recordShown,
    });
  } catch {
    return fallback("本地数据库暂时不可用，已切换到只读演示内容。");
  }
}

export async function nextCard(currentId: string): Promise<KnowledgeCard> {
  return desktopOr("next_card", { currentId }, () => {
    const card = visibleFallbackCard(currentId);
    if (!card) throw new Error("没有可显示的本地知识卡");
    const result = withFavorite(card);
    addToHistory(result);
    return result;
  });
}

export async function availableCardCount(): Promise<number> {
  return desktopOr(
    "available_card_count",
    {},
    () =>
      fallbackCards.filter(
        (card) => !memory.hidden.has(card.id) && !memory.deleted.has(card.id),
      ).length,
  );
}

export async function recordInteraction(
  cardId: string,
  kind: InteractionKind,
): Promise<void> {
  return desktopOr("record_interaction", { cardId, kind }, () => {
    if (kind === "favorited") memory.favorites.add(cardId);
    if (kind === "unfavorited") memory.favorites.delete(cardId);
    if (kind === "disliked" || kind === "known") memory.hidden.add(cardId);
    if (kind === "known") memory.favorites.delete(cardId);
    memory.history = memory.history.map((item) => ({
      ...item,
      card: withFavorite(item.card),
    }));
  });
}

export async function recordCardShown(cardId: string): Promise<void> {
  return desktopOr("record_interaction", { cardId, kind: "shown" }, () => {
    const card = fallbackCards.find((item) => item.id === cardId);
    if (card) addToHistory(card);
  });
}

export async function saveOnboarding(
  input: OnboardingInput,
): Promise<TopicPreference[]> {
  return desktopOr("save_onboarding", { input }, () => {
    memory.onboardingComplete = true;
    memory.settings.reminderPreset = input.reminderPreset;
    memory.topics = memory.topics.map((topic) => ({
      ...topic,
      selected: input.selectedTopicIds.includes(topic.id),
    }));
    input.customInterests.forEach((label, index) => {
      memory.topics.push({
        id: `custom-${index}-${label}`,
        label,
        selected: true,
        enabled: true,
        custom: true,
        rank: memory.topics.length,
        weight: 0,
      });
    });
    return memory.topics.map((topic) => ({ ...topic }));
  });
}

export async function saveInterests(
  topics: TopicPreference[],
  personalizationEnabled: boolean,
): Promise<TopicPreference[]> {
  return desktopOr("save_interests", { topics, personalizationEnabled }, () => {
    memory.topics = topics.map((topic) => ({ ...topic }));
    memory.settings.personalizationEnabled = personalizationEnabled;
    return memory.topics;
  });
}

export async function listLibrary(
  mode: "favorites" | "history",
  topicId: string | null,
  sort: "newest" | "oldest",
): Promise<LibraryItem[]> {
  return desktopOr("list_library", { mode, topicId, sort }, () => {
    let result = memory.history.filter(
      (item) =>
        !memory.deleted.has(item.card.id) &&
        (!topicId || item.card.topicId === topicId) &&
        (mode === "history" || memory.favorites.has(item.card.id)),
    );
    if (sort === "oldest") result = [...result].reverse();
    return result.map((item) => ({ ...item, card: withFavorite(item.card) }));
  });
}

export async function deleteLibraryCard(cardId: string): Promise<void> {
  return desktopOr("delete_library_card", { cardId }, () => {
    memory.deleted.add(cardId);
    memory.hidden.add(cardId);
    memory.favorites.delete(cardId);
    memory.history = memory.history.filter((item) => item.card.id !== cardId);
    memory.followUps.delete(cardId);
  });
}

export async function saveSettings(
  settings: AppSettings,
): Promise<AppSettings> {
  return desktopOr("save_settings", { settings }, () => {
    memory.settings = { ...settings };
    return { ...memory.settings };
  });
}

export async function generationUsage(): Promise<GenerationUsage> {
  return desktopOr("generation_usage", {}, () => ({
    generated: 0,
    total: memory.settings.dailyGenerationLimit,
  }));
}

export async function pauseReminders(
  mode: "thirty_minutes" | "today",
): Promise<string> {
  return desktopOr("pause_reminders", { mode }, () => {
    const until = new Date();
    if (mode === "thirty_minutes") until.setMinutes(until.getMinutes() + 30);
    else until.setHours(24, 0, 0, 0);
    memory.settings.pauseUntil = until.toISOString();
    return until.toISOString();
  });
}

export const CREDENTIAL_REPLACEMENT_REQUIRED =
  "目标服务通道已有 API Key，请确认覆盖后重试";

export async function saveProviderProfile(input: {
  providerId: string;
  region: string;
  model: string;
  apiKey: string | null;
  replaceExistingKey: boolean;
}): Promise<ProviderSpec> {
  return desktopOr("save_provider_profile", { input }, () => {
    throw new Error("浏览器预览不保存 API Key。请在桌面应用中配置。 ");
  });
}

export async function deleteProviderKey(
  providerId: string,
  region: string,
): Promise<void> {
  return desktopOr(
    "delete_provider_key",
    { providerId, region },
    () => undefined,
  );
}

export async function testProviderConnection(
  providerId: string,
  region: string,
): Promise<string> {
  return desktopOr("test_provider_connection", { providerId, region }, () => {
    throw new Error("浏览器预览不会发起模型请求，请在桌面应用中测试连接。");
  });
}

export async function saveGenerationProvider(
  providerId: "deepseek" | "kimi",
): Promise<"deepseek" | "kimi"> {
  return desktopOr("save_generation_provider", { providerId }, () => {
    memory.generationProviderId = providerId;
    return providerId;
  });
}

export async function generateSameTopic(
  cardId: string,
): Promise<KnowledgeCard> {
  return desktopOr("generate_same_topic", { cardId }, () => {
    throw new Error("请先配置模型哦~");
  });
}

export async function askFollowUp(
  cardId: string,
  question: string,
  history: FollowUpTurn[],
  displayQuestion = question,
  searchRunId?: string,
): Promise<FollowUpResult> {
  return desktopOr(
    "ask_follow_up",
    {
      cardId,
      question,
      displayQuestion,
      history,
      ...(searchRunId ? { searchRunId } : {}),
    },
    () => {
      throw new Error("请先配置模型哦~");
    },
  );
}

export async function listCardFollowUps(
  cardId: string,
): Promise<FollowUpMessage[]> {
  return desktopOr("list_card_follow_ups", { cardId }, () =>
    structuredClone(memory.followUps.get(cardId) ?? []),
  );
}

export async function generateTopicBatch(
  topicId: string | null,
  topicLabel: string,
  count: number,
): Promise<GenerationBatchResult> {
  return desktopOr(
    "generate_topic_batch",
    { topicId, topicLabel, count },
    () => {
      throw new Error("请先配置模型哦~");
    },
  );
}

export async function generateRandomTopic(): Promise<KnowledgeCard> {
  return desktopOr("generate_random_topic", {}, () => {
    throw new Error("请先配置模型哦~");
  });
}

export async function generateRandomTopicBatch(
  count: number,
): Promise<GenerationBatchResult> {
  return desktopOr("generate_random_topic_batch", { count }, () => {
    throw new Error("请先配置模型哦~");
  });
}

export async function chooseAndImportCards(): Promise<ImportResult | null> {
  if (!isDesktop()) throw new Error("知识卡导入只在桌面应用中可用。");
  const path = await withAutoHideGuard("file-picker", () =>
    open({
      multiple: false,
      directory: false,
      filters: [{ name: "知识卡文件", extensions: ["json", "csv"] }],
    }),
  );
  if (!path) return null;
  return invoke<ImportResult>("import_cards_file", { path });
}

export async function clearData(scope: DataClearScope): Promise<string | null> {
  return desktopOr("clear_data", { scope }, () => {
    if (scope === "history" || scope === "all") memory.history = [];
    if (scope === "preferences") {
      memory.topics = memory.topics
        .filter((topic) => !topic.custom)
        .map((topic) => ({ ...topic, weight: 0 }));
    }
    if (scope === "all") {
      memory.topics = memory.topics
        .filter((topic) => !topic.custom)
        .map((topic) => ({
          ...topic,
          selected: false,
          enabled: true,
          weight: 0,
        }));
      memory.favorites.clear();
      memory.hidden.clear();
      memory.deleted.clear();
      memory.followUps.clear();
      memory.onboardingComplete = false;
      memory.settings = { ...defaultSettings };
      memory.generationProviderId = "deepseek";
    }
    return null;
  });
}

export async function openSourceUrl(url: string): Promise<void> {
  return desktopOr("open_source_url", { url }, () => {
    window.open(url, "_blank", "noopener,noreferrer");
  });
}

export async function exitApplication(): Promise<void> {
  return desktopOr("exit_application", {}, () => undefined);
}

export function friendlyError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "操作没有完成，请稍后再试。";
}
