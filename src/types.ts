export type TrustStatus =
  "verified" | "source_grounded" | "ai_unverified" | "demo_unreviewed";

export type Difficulty = "beginner" | "general" | "advanced";

export interface SourceRef {
  title: string;
  url: string;
  publisher?: string;
  accessedAt?: string;
}

export interface KnowledgeCard {
  id: string;
  schemaVersion: number;
  language: "zh-CN";
  topicId: string;
  topicLabel: string;
  tags: string[];
  question: string;
  shortAnswer: string;
  explanation: string;
  whyItMatters?: string;
  difficulty: Difficulty;
  estimatedReadSeconds: number;
  sourceRefs: SourceRef[];
  trustStatus: TrustStatus;
  contentFingerprint: string;
  generatedBy?: {
    provider: string;
    model: string;
    promptVersion: string;
  };
  createdAt: string;
  isFavorite: boolean;
  hiddenFromFeed?: boolean;
}

export interface TopicPreference {
  id: string;
  label: string;
  selected: boolean;
  enabled: boolean;
  custom: boolean;
  rank: number;
  weight: number;
}

export type ThemeMode = "system" | "light" | "dark";
export type ReminderPreset =
  "manual" | "weekday_once" | "weekday_twice" | "custom";

export interface AppSettings {
  theme: ThemeMode;
  alwaysOnTop: boolean;
  autostart: boolean;
  personalizationEnabled: boolean;
  reminderPreset: ReminderPreset;
  reminderTimes: string[];
  weekdays: number[];
  quietStart: string;
  quietEnd: string;
  pauseUntil: string | null;
  globalShortcut: string;
  dailyGenerationLimit: number;
}

export interface OnboardingInput {
  selectedTopicIds: string[];
  customInterests: string[];
  reminderPreset: ReminderPreset;
}

export type InteractionKind =
  "revealed" | "expanded" | "disliked" | "known" | "favorited" | "unfavorited";

export interface FollowUpTurn {
  role: "user" | "assistant";
  content: string;
}

export interface FollowUpResult {
  answer: string;
  providerId: "deepseek" | "kimi";
  model: string;
  switchedFromProviderId: "deepseek" | "kimi" | null;
}

export interface LibraryItem {
  card: KnowledgeCard;
  viewedAt: string;
}

export interface ProviderModel {
  id: string;
  label: string;
  recommended: boolean;
}

export interface ProviderRegion {
  id: string;
  label: string;
}

export interface ProviderSpec {
  id: "deepseek" | "kimi";
  label: string;
  regions: ProviderRegion[];
  models: ProviderModel[];
  selectedRegion: string;
  selectedModel: string;
  keyConfigured: boolean;
  keyLast4: string | null;
  connectionVerified: boolean;
}

export interface BootstrapData {
  onboardingComplete: boolean;
  card: KnowledgeCard | null;
  availableCardCount: number;
  topics: TopicPreference[];
  settings: AppSettings;
  providers: ProviderSpec[];
  generationProviderId: "deepseek" | "kimi";
  desktopCapabilities: {
    tray: boolean;
    notifications: boolean;
    secureCredentials: boolean;
  };
  notice: string | null;
}

export interface ImportResult {
  imported: number;
  duplicates: number;
  rejected: number;
  errors: string[];
}

export interface GenerationUsage {
  generated: number;
  total: number;
}

export interface GenerationBatchResult {
  cards: KnowledgeCard[];
  requestedCount: number;
  warning: string | null;
  providerId: "deepseek" | "kimi";
  switchedFromProviderId: "deepseek" | "kimi" | null;
}

export type DataClearScope = "history" | "preferences" | "all";
