import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AppHeader, type AppView } from "./components/AppHeader";
import { KnowledgeCardView } from "./components/KnowledgeCardView";
import { KnowledgeHome } from "./components/KnowledgeHome";
import { Onboarding } from "./components/Onboarding";
import {
  askFollowUp,
  availableCardCount,
  bootstrapApp,
  friendlyError,
  generateRandomTopic,
  generateRandomTopicBatch,
  generateSameTopic,
  generateTopicBatch,
  isDesktop,
  listCardFollowUps,
  nextCard,
  recordCardShown,
  recordInteraction,
  saveGenerationProvider,
  saveOnboarding,
} from "./lib/api";
import {
  installAutoHideInteractionGuards,
  resetAutoHideGuards,
} from "./lib/autoHideGuard";
import { GeneralSettingsScreen } from "./screens/GeneralSettingsScreen";
import { InterestSettingsScreen } from "./screens/InterestSettingsScreen";
import { LibraryScreen } from "./screens/LibraryScreen";
import { KnowledgeBaseScreen } from "./screens/KnowledgeBaseScreen";
import { ModelSettingsScreen } from "./screens/ModelSettingsScreen";
import type {
  AppSettings,
  BootstrapData,
  DataClearScope,
  FollowUpResult,
  FollowUpTurn,
  GenerationBatchResult,
  InteractionKind,
  KnowledgeCard,
  ProviderSpec,
  TopicPreference,
} from "./types";

const NO_MORE_CARDS_NOTICE = "当前没有知识点啦，快去生成吧~";
const FOLLOW_UP_NAVIGATION_NOTICE =
  "AI 正在回答，请等待完成后再离开当前知识卡。";
const BUSY_NAVIGATION_NOTICE = "当前操作正在进行，请等待完成后再切换页面。";

interface PendingGeneration {
  topicId: string;
  topicLabel: string;
  completed: number;
  total: number;
  remaining: number;
}

function generationProgressMessage(
  result: GenerationBatchResult,
  completed: number,
  total: number,
  topicLabel: string,
) {
  const summary = `已生成 ${completed}/${total} 条新的${topicLabel}知识点。`;
  if (completed >= total) {
    return result.warning ? `${summary} ${result.warning}。` : summary;
  }
  const warning = result.warning ?? "模型未完成后续批次";
  return `${summary} 剩余 ${total - completed} 条未完成：${warning}。可返回主界面继续生成。`;
}

export default function App() {
  const [data, setData] = useState<BootstrapData | null>(null);
  const [view, setView] = useState<AppView>("home");
  const [homeMode, setHomeMode] = useState<"landing" | "card">("landing");
  const [menuOpen, setMenuOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [cardFollowUpBusy, setCardFollowUpBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [libraryRefresh, setLibraryRefresh] = useState(0);
  const [online, setOnline] = useState(navigator.onLine);
  const [backStack, setBackStack] = useState<KnowledgeCard[]>([]);
  const [forwardStack, setForwardStack] = useState<KnowledgeCard[]>([]);
  const [dismissedCardIds, setDismissedCardIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [pendingGeneration, setPendingGeneration] =
    useState<PendingGeneration | null>(null);
  const [floatingNotice, setFloatingNotice] = useState<{
    id: number;
    text: string;
  } | null>(null);
  const busyRef = useRef(false);
  const cardFollowUpBusyRef = useRef(false);
  const cardNavigationLocked =
    cardFollowUpBusy && view === "home" && homeMode === "card";
  const navigationLocked = busy || cardNavigationLocked;
  const updateCardFollowUpBusy = useCallback((value: boolean) => {
    cardFollowUpBusyRef.current = value;
    setCardFollowUpBusy(value);
  }, []);

  useEffect(() => {
    void resetAutoHideGuards();
    const cleanupInteractionGuards = installAutoHideInteractionGuards();
    return () => {
      cleanupInteractionGuards();
      void resetAutoHideGuards();
    };
  }, []);

  useEffect(() => {
    busyRef.current = busy;
  }, [busy]);

  useEffect(() => {
    let active = true;
    void bootstrapApp({ recordShown: false })
      .then((bootstrap) => {
        if (active) {
          setData(bootstrap);
          setMessage(bootstrap.notice);
        }
      })
      .catch((error: unknown) => {
        if (active) setMessage(friendlyError(error));
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    const handleOnline = () => setOnline(true);
    const handleOffline = () => setOnline(false);
    window.addEventListener("online", handleOnline);
    window.addEventListener("offline", handleOffline);
    return () => {
      window.removeEventListener("online", handleOnline);
      window.removeEventListener("offline", handleOffline);
    };
  }, []);

  useEffect(() => {
    if (!data) return;
    document.documentElement.dataset.theme = data.settings.theme;
  }, [data]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented) return;
      if (event.key === "Escape") {
        if (menuOpen) setMenuOpen(false);
        else if (navigationLocked) {
          event.preventDefault();
          setMessage(
            cardNavigationLocked
              ? FOLLOW_UP_NAVIGATION_NOTICE
              : BUSY_NAVIGATION_NOTICE,
          );
        } else if (view !== "home") setView("home");
        else if (homeMode === "card") setHomeMode("landing");
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [cardNavigationLocked, homeMode, menuOpen, navigationLocked, view]);

  useEffect(() => {
    if (!message) return;
    const timeout = window.setTimeout(() => setMessage(null), 5200);
    return () => window.clearTimeout(timeout);
  }, [message]);

  useEffect(() => {
    if (!floatingNotice) return;
    const timeout = window.setTimeout(() => setFloatingNotice(null), 1000);
    return () => window.clearTimeout(timeout);
  }, [floatingNotice]);

  useEffect(() => {
    if (!isDesktop()) return;
    const cleanups = Promise.all([
      listen("desktop-notice", (event) => setMessage(String(event.payload))),
      listen("open-settings", () => {
        if (busyRef.current || cardFollowUpBusyRef.current) {
          setMessage(
            cardFollowUpBusyRef.current
              ? FOLLOW_UP_NAVIGATION_NOTICE
              : BUSY_NAVIGATION_NOTICE,
          );
          return;
        }
        setView("settings");
        setMenuOpen(false);
      }),
      listen("request-next-card", () => {
        if (busyRef.current || cardFollowUpBusyRef.current) {
          setMessage(
            cardFollowUpBusyRef.current
              ? FOLLOW_UP_NAVIGATION_NOTICE
              : BUSY_NAVIGATION_NOTICE,
          );
          return;
        }
        setData((current) => {
          if (!current?.card) return current;
          void nextCard(current.card.id)
            .then((card) => {
              setData((latest) => (latest ? { ...latest, card } : latest));
              setHomeMode("card");
              setView("home");
              setLibraryRefresh((value) => value + 1);
            })
            .catch((error: unknown) => setMessage(friendlyError(error)));
          return current;
        });
      }),
    ]);
    return () => {
      void cleanups.then((functions) =>
        functions.forEach((cleanup) => cleanup()),
      );
    };
  }, []);

  function navigate(target: AppView) {
    if (busyRef.current) {
      setMessage(BUSY_NAVIGATION_NOTICE);
      setMenuOpen(false);
      return;
    }
    if (cardFollowUpBusyRef.current && view === "home" && homeMode === "card") {
      setMessage(FOLLOW_UP_NAVIGATION_NOTICE);
      setMenuOpen(false);
      return;
    }
    setView(target);
    if (target === "home") setHomeMode("landing");
    setMenuOpen(false);
  }

  function refreshAvailableCardCount() {
    void availableCardCount()
      .then((count) =>
        setData((current) =>
          current ? { ...current, availableCardCount: count } : current,
        ),
      )
      .catch(() => undefined);
  }

  function showFloatingNotice(text: string) {
    setFloatingNotice((current) => ({
      id: (current?.id ?? 0) + 1,
      text,
    }));
  }

  async function browseAvailableCard() {
    if (!data || busy || data.availableCardCount === 0) return;
    if (
      data.card &&
      !data.card.hiddenFromFeed &&
      !dismissedCardIds.has(data.card.id)
    ) {
      setBusy(true);
      try {
        await recordCardShown(data.card.id);
        setHomeMode("card");
        setLibraryRefresh((value) => value + 1);
      } catch (error) {
        setMessage(friendlyError(error));
      } finally {
        setBusy(false);
      }
      return;
    }
    setBusy(true);
    try {
      const card = await nextCard(data.card?.id ?? "");
      setData((current) => (current ? { ...current, card } : current));
      setHomeMode("card");
      setLibraryRefresh((value) => value + 1);
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(false);
    }
  }

  async function completeOnboarding(
    input: Parameters<typeof saveOnboarding>[0],
  ) {
    if (!data) return;
    setBusy(true);
    try {
      const topics = await saveOnboarding(input);
      setData({
        ...data,
        onboardingComplete: true,
        topics,
        settings: { ...data.settings, reminderPreset: input.reminderPreset },
      });
      setMessage("准备好了。可以开始浏览本地知识。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(false);
    }
  }

  async function interaction(kind: InteractionKind) {
    if (!data?.card) return;
    const activeCard = data.card;
    try {
      await recordInteraction(activeCard.id, kind);
      if (kind === "favorited" || kind === "unfavorited") {
        setData((current) =>
          current
            ? {
                ...current,
                card: {
                  ...activeCard,
                  isFavorite: kind === "favorited",
                },
              }
            : current,
        );
        setLibraryRefresh((value) => value + 1);
      }
    } catch (error) {
      setMessage(friendlyError(error));
    }
  }

  async function askCurrentCardFollowUp(
    question: string,
    history: FollowUpTurn[],
    displayQuestion: string,
  ): Promise<FollowUpResult> {
    if (!data?.card) throw new Error("当前知识卡不可用，请切换后再试");
    return askFollowUp(data.card.id, question, history, displayQuestion);
  }

  async function advance() {
    if (!data?.card || busy) return;
    if (data.availableCardCount <= 1) {
      showFloatingNotice(NO_MORE_CARDS_NOTICE);
      return;
    }
    const activeCard = data.card;
    const visibleForward = forwardStack.filter(
      (card) => !card.hiddenFromFeed && !dismissedCardIds.has(card.id),
    );
    const forward = visibleForward.at(-1);
    if (forward) {
      setBackStack((cards) =>
        [
          ...cards.filter(
            (card) => !card.hiddenFromFeed && !dismissedCardIds.has(card.id),
          ),
          ...(activeCard.hiddenFromFeed || dismissedCardIds.has(activeCard.id)
            ? []
            : [activeCard]),
        ].slice(-100),
      );
      setForwardStack(visibleForward.slice(0, -1));
      setData({ ...data, card: forward });
      return;
    }
    setBusy(true);
    try {
      const card = await nextCard(activeCard.id);
      setBackStack((cards) =>
        [
          ...cards.filter(
            (item) => !item.hiddenFromFeed && !dismissedCardIds.has(item.id),
          ),
          ...(activeCard.hiddenFromFeed || dismissedCardIds.has(activeCard.id)
            ? []
            : [activeCard]),
        ].slice(-100),
      );
      setForwardStack([]);
      setData((current) => (current ? { ...current, card } : current));
      setLibraryRefresh((value) => value + 1);
    } catch (error) {
      setMessage(`${friendlyError(error)} 仍可继续阅读当前卡片。`);
    } finally {
      setBusy(false);
    }
  }

  function previous() {
    if (!data?.card || busy) return;
    if (data.availableCardCount <= 1) {
      showFloatingNotice(NO_MORE_CARDS_NOTICE);
      return;
    }
    const activeCard = data.card;
    const visibleBack = backStack.filter(
      (card) => !card.hiddenFromFeed && !dismissedCardIds.has(card.id),
    );
    const card = visibleBack.at(-1);
    if (!card) return;
    setBackStack(visibleBack.slice(0, -1));
    setForwardStack((cards) =>
      [
        ...cards.filter(
          (item) => !item.hiddenFromFeed && !dismissedCardIds.has(item.id),
        ),
        ...(activeCard.hiddenFromFeed || dismissedCardIds.has(activeCard.id)
          ? []
          : [activeCard]),
      ].slice(-100),
    );
    setData({ ...data, card });
  }

  async function removeCurrentFromFeed(
    kind: "disliked" | "known",
    successMessage: string,
  ) {
    if (!data?.card || busy) return;
    const dismissedId = data.card.id;
    setBusy(true);
    try {
      await recordInteraction(dismissedId, kind);
      const excludedIds = new Set(dismissedCardIds);
      excludedIds.add(dismissedId);
      const visibleForward = forwardStack.filter(
        (card) =>
          !card.hiddenFromFeed &&
          card.id !== dismissedId &&
          !excludedIds.has(card.id),
      );
      let card = visibleForward.at(-1) ?? null;
      setDismissedCardIds((ids) => {
        const next = new Set(ids);
        next.add(dismissedId);
        return next;
      });
      setBackStack((cards) =>
        cards.filter(
          (item) => item.id !== dismissedId && !dismissedCardIds.has(item.id),
        ),
      );
      setForwardStack(visibleForward.slice(0, -1));
      if (!card) {
        try {
          card = await nextCard(dismissedId);
        } catch (error) {
          if (friendlyError(error) !== "没有可显示的本地知识卡") {
            setMessage(friendlyError(error));
          }
        }
      }
      setData((current) =>
        current
          ? {
              ...current,
              card,
              availableCardCount: Math.max(0, current.availableCardCount - 1),
            }
          : current,
      );
      refreshAvailableCardCount();
      setLibraryRefresh((value) => value + 1);
      if (card) setMessage(successMessage);
      else setHomeMode("landing");
    } catch (error) {
      const text = friendlyError(error);
      setMessage(text);
      throw new Error(text);
    } finally {
      setBusy(false);
    }
  }

  async function dismissCurrent() {
    await removeCurrentFromFeed(
      "disliked",
      "已减少这个领域的推荐，正在展示下一条。",
    );
  }

  async function masterCurrent() {
    await removeCurrentFromFeed("known", "已记录这次收获，正在展示下一条。");
  }

  async function generateFromCurrentTopic() {
    if (!data?.card || busy) return;
    const activeCard = data.card;
    if (!data.providers.some((provider) => provider.keyConfigured)) {
      showFloatingNotice("请先配置模型哦~");
      return;
    }
    setBusy(true);
    try {
      const card = await generateSameTopic(activeCard.id);
      setBackStack((cards) => [...cards, activeCard].slice(-100));
      setForwardStack([]);
      setData((current) =>
        current
          ? {
              ...current,
              card,
              availableCardCount: current.availableCardCount + 1,
            }
          : current,
      );
      setHomeMode("card");
      refreshAvailableCardCount();
      setLibraryRefresh((value) => value + 1);
      setMessage("已生成一条新的" + activeCard.topicLabel + "知识点。");
    } catch (error) {
      const text = friendlyError(error);
      if (text === "请先配置模型哦~") showFloatingNotice(text);
      else setMessage(text);
    } finally {
      setBusy(false);
    }
  }

  async function generateFromRandomTopic() {
    if (!data || busy) return;
    if (!data.providers.some((provider) => provider.keyConfigured)) {
      showFloatingNotice("请先配置模型哦~");
      return;
    }
    const activeCard = data.card;
    setBusy(true);
    try {
      const card = await generateRandomTopic();
      if (activeCard && !activeCard.hiddenFromFeed) {
        setBackStack((cards) => [...cards, activeCard].slice(-100));
      } else {
        setBackStack([]);
      }
      setForwardStack([]);
      setData((current) =>
        current
          ? {
              ...current,
              card,
              availableCardCount: current.availableCardCount + 1,
            }
          : current,
      );
      setHomeMode("card");
      refreshAvailableCardCount();
      setLibraryRefresh((value) => value + 1);
      setMessage("已按兴趣权重随机生成一条" + card.topicLabel + "知识点。");
    } catch (error) {
      const text = friendlyError(error);
      if (text === "请先配置模型哦~") showFloatingNotice(text);
      else setMessage(text);
    } finally {
      setBusy(false);
    }
  }

  async function generateBatchFromRandomTopic(count: number) {
    if (!data || busy) return;
    if (!data.providers.some((provider) => provider.keyConfigured)) {
      showFloatingNotice("请先配置模型哦~");
      return;
    }
    const activeCard = data.card;
    setPendingGeneration(null);
    setBusy(true);
    try {
      const result = await generateRandomTopicBatch(count);
      const cards = result.cards;
      const card = cards[0];
      if (!card) throw new Error("模型没有返回可用的知识点");
      if (activeCard && !activeCard.hiddenFromFeed) {
        setBackStack((items) => [...items, activeCard].slice(-100));
      } else {
        setBackStack([]);
      }
      setForwardStack([]);
      setData((current) =>
        current
          ? {
              ...current,
              card,
              availableCardCount: current.availableCardCount + cards.length,
            }
          : current,
      );
      setHomeMode("card");
      refreshAvailableCardCount();
      setLibraryRefresh((value) => value + 1);
      const completed = cards.length;
      const remaining = Math.max(0, result.requestedCount - completed);
      setPendingGeneration(
        remaining > 0
          ? {
              topicId: card.topicId,
              topicLabel: card.topicLabel,
              completed,
              total: result.requestedCount,
              remaining,
            }
          : null,
      );
      setMessage(
        generationProgressMessage(
          result,
          completed,
          result.requestedCount,
          card.topicLabel,
        ),
      );
    } catch (error) {
      const text = friendlyError(error);
      if (text === "请先配置模型哦~") showFloatingNotice(text);
      else setMessage(text);
    } finally {
      setBusy(false);
    }
  }

  async function selectGenerationProvider(providerId: "deepseek" | "kimi") {
    if (!data || busy || providerId === data.generationProviderId) return;
    setBusy(true);
    try {
      const saved = await saveGenerationProvider(providerId);
      const selected = data.providers.find((provider) => provider.id === saved);
      const fallback = data.providers.find(
        (provider) => provider.id !== saved && provider.connectionVerified,
      );
      setData((current) =>
        current ? { ...current, generationProviderId: saved } : current,
      );
      if (!selected?.connectionVerified && fallback) {
        setMessage(
          `已选择${selected?.label ?? saved}；其未就绪时会自动切换到${fallback.label}。`,
        );
      } else {
        setMessage(`已将${selected?.label ?? saved}设为首选生成模型。`);
      }
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(false);
    }
  }

  async function generateFromSelectedTopic(
    topicId: string | null,
    topicLabel: string,
    count: number,
    progress?: PendingGeneration,
  ) {
    if (!data || busy) return;
    if (!data.providers.some((provider) => provider.keyConfigured)) {
      showFloatingNotice("请先配置模型哦~");
      return;
    }
    if (!progress) setPendingGeneration(null);
    setBusy(true);
    try {
      const result = await generateTopicBatch(topicId, topicLabel, count);
      const cards = result.cards;
      const card = cards[0];
      if (!card) throw new Error("模型没有返回可用的知识点");
      setData((current) => {
        if (!current) return current;
        const topics = current.topics.some((topic) => topic.id === card.topicId)
          ? current.topics
          : [
              ...current.topics,
              {
                id: card.topicId,
                label: card.topicLabel,
                selected: false,
                enabled: true,
                custom: true,
                rank: current.topics.length,
                weight: 0,
              },
            ];
        return {
          ...current,
          card,
          topics,
          availableCardCount: current.availableCardCount + cards.length,
        };
      });
      setHomeMode("card");
      refreshAvailableCardCount();
      setBackStack([]);
      setForwardStack([]);
      setLibraryRefresh((value) => value + 1);
      const total = progress?.total ?? result.requestedCount;
      const completed = (progress?.completed ?? 0) + cards.length;
      const remaining = Math.max(0, total - completed);
      setPendingGeneration(
        remaining > 0
          ? {
              topicId: card.topicId,
              topicLabel: card.topicLabel,
              completed,
              total,
              remaining,
            }
          : null,
      );
      setMessage(
        generationProgressMessage(result, completed, total, card.topicLabel),
      );
    } catch (error) {
      const text = friendlyError(error);
      if (text === "请先配置模型哦~") showFloatingNotice(text);
      else setMessage(text);
    } finally {
      setBusy(false);
    }
  }

  function openCard(card: KnowledgeCard) {
    if (!data) return;
    if (card.hiddenFromFeed) {
      setDismissedCardIds((ids) => {
        const next = new Set(ids);
        next.add(card.id);
        return next;
      });
    }
    if (
      data.card &&
      !data.card.hiddenFromFeed &&
      !dismissedCardIds.has(data.card.id)
    ) {
      setBackStack((cards) => [...cards, data.card!].slice(-100));
    }
    setForwardStack([]);
    setData({ ...data, card });
    setHomeMode("card");
    setView("home");
    setMenuOpen(false);
  }

  function cardDeleted(cardId: string) {
    setDismissedCardIds((ids) => {
      const next = new Set(ids);
      next.add(cardId);
      return next;
    });
    setBackStack((cards) => cards.filter((card) => card.id !== cardId));
    setForwardStack((cards) => cards.filter((card) => card.id !== cardId));
    setLibraryRefresh((value) => value + 1);
    refreshAvailableCardCount();
    if (data?.card?.id !== cardId) return;
    setData({ ...data, card: null });
    void nextCard(cardId)
      .then((card) =>
        setData((current) => (current ? { ...current, card } : current)),
      )
      .catch((error: unknown) => {
        setHomeMode("landing");
        if (friendlyError(error) !== "没有可显示的本地知识卡") {
          setMessage(friendlyError(error));
        }
      });
  }

  function updateTopics(topics: TopicPreference[], personalization: boolean) {
    if (!data) return;
    setData({
      ...data,
      topics,
      settings: { ...data.settings, personalizationEnabled: personalization },
    });
  }

  function updateSettings(settings: AppSettings) {
    if (!data) return;
    setData({ ...data, settings });
  }

  function updateProviders(providers: ProviderSpec[]) {
    if (!data) return;
    setData({ ...data, providers });
  }

  async function dataCleared(
    scope: DataClearScope,
    warning: string | null = null,
  ) {
    setLibraryRefresh((value) => value + 1);
    if (scope === "history" || scope === "all") {
      setBackStack([]);
      setForwardStack([]);
    }
    if (scope === "all") {
      setView("home");
      setHomeMode("landing");
      setDismissedCardIds(new Set());
    }
    if (scope === "preferences" || scope === "all") {
      try {
        setData(await bootstrapApp({ recordShown: false }));
      } catch (error) {
        setMessage(friendlyError(error));
      }
    }
    if (warning) setMessage(warning);
  }

  if (!data) {
    return (
      <main className="startup" aria-busy="true" aria-live="polite">
        <div className="startup-brand">
          <span>T</span> Tell You Why
        </div>
        <div className="question-skeleton" />
        <p>{message ?? "正在准备本地知识…"}</p>
      </main>
    );
  }

  if (!data.onboardingComplete && data.card) {
    return (
      <div className="app-shell onboarding-shell">
        <Onboarding
          topics={data.topics}
          previewCard={data.card}
          busy={busy}
          onComplete={completeOnboarding}
        />
        {message ? (
          <div className="toast" role="status">
            {message}
          </div>
        ) : null}
      </div>
    );
  }

  return (
    <div className="app-shell">
      <AppHeader
        view={view}
        menuOpen={menuOpen}
        navigationLocked={navigationLocked}
        onMenuToggle={() => setMenuOpen((value) => !value)}
        onNavigate={navigate}
      />
      {!online ? (
        <div className="offline-banner" role="status">
          当前离线，正在使用本地内容
        </div>
      ) : null}
      {view === "home" && homeMode === "card" && data.card ? (
        <KnowledgeCardView
          key={data.card.id}
          card={data.card}
          availableCardCount={data.availableCardCount}
          busy={busy}
          canGoPrevious={backStack.some(
            (card) => !dismissedCardIds.has(card.id),
          )}
          onAskFollowUp={askCurrentCardFollowUp}
          onLoadFollowUps={listCardFollowUps}
          onFollowUpBusyChange={updateCardFollowUpBusy}
          onInteraction={interaction}
          onReturnHome={() => {
            if (!cardFollowUpBusyRef.current) setHomeMode("landing");
          }}
          onPrevious={previous}
          onNext={advance}
          onDismiss={dismissCurrent}
          onMaster={masterCurrent}
          onGenerateSameTopic={generateFromCurrentTopic}
          onGenerateRandomTopic={generateFromRandomTopic}
        />
      ) : null}
      {view === "home" && (homeMode === "landing" || !data.card) ? (
        <KnowledgeHome
          topics={data.topics}
          providers={data.providers}
          generationProviderId={data.generationProviderId}
          availableCardCount={data.availableCardCount}
          maxGenerationCount={data.settings.dailyGenerationLimit}
          busy={busy}
          pendingGeneration={pendingGeneration}
          onBrowse={browseAvailableCard}
          onGenerate={generateFromSelectedTopic}
          onGenerateRandom={generateBatchFromRandomTopic}
          onOpenModelSettings={() => navigate("models")}
          onGenerationProviderChange={selectGenerationProvider}
          onContinueGeneration={async () => {
            if (!pendingGeneration) return;
            await generateFromSelectedTopic(
              pendingGeneration.topicId,
              pendingGeneration.topicLabel,
              pendingGeneration.remaining,
              pendingGeneration,
            );
          }}
        />
      ) : null}
      {view === "library" ? (
        <LibraryScreen
          topics={data.topics}
          refreshToken={libraryRefresh}
          onOpenCard={openCard}
          onCardDeleted={cardDeleted}
          onHistoryCleared={() => void dataCleared("history")}
        />
      ) : null}
      {view === "knowledge-base" ? <KnowledgeBaseScreen /> : null}
      {view === "interests" ? (
        <InterestSettingsScreen
          initialTopics={data.topics}
          personalizationEnabled={data.settings.personalizationEnabled}
          onSaved={updateTopics}
        />
      ) : null}
      {view === "models" ? (
        <ModelSettingsScreen
          initialProviders={data.providers}
          onProvidersChanged={updateProviders}
        />
      ) : null}
      {view === "settings" ? (
        <GeneralSettingsScreen
          initialSettings={data.settings}
          onSaved={updateSettings}
          onDataCleared={dataCleared}
        />
      ) : null}
      {floatingNotice ? (
        <div key={floatingNotice.id} className="floating-notice" role="status">
          {floatingNotice.text}
        </div>
      ) : null}
      {message ? (
        <div className="toast" role="status">
          {message}
        </div>
      ) : null}
    </div>
  );
}
