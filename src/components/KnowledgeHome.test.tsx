import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { presetTopics } from "../data/fallbackCards";
import type { ProviderSpec, TopicPreference } from "../types";
import { KnowledgeHome } from "./KnowledgeHome";

const topics = presetTopics.map<TopicPreference>(([id, label], rank) => ({
  id,
  label,
  selected: rank === 2,
  enabled: true,
  custom: false,
  rank,
  weight: 0,
}));

const providers: ProviderSpec[] = [
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
    ],
    selectedRegion: "default",
    selectedModel: "deepseek-v4-flash",
    keyConfigured: true,
    keyLast4: "1234",
    connectionVerified: true,
  },
  {
    id: "kimi",
    label: "Kimi",
    regions: [{ id: "cn", label: "中国大陆" }],
    models: [{ id: "kimi-k3", label: "Kimi K3", recommended: true }],
    selectedRegion: "cn",
    selectedModel: "kimi-k3",
    keyConfigured: false,
    keyLast4: null,
    connectionVerified: false,
  },
];

describe("KnowledgeHome", () => {
  it("opens local cards, prioritizes interests, and supports a custom topic", async () => {
    const user = userEvent.setup();
    const onBrowse = vi.fn().mockResolvedValue(undefined);
    const onGenerate = vi.fn().mockResolvedValue(undefined);
    const onGenerateRandom = vi.fn().mockResolvedValue(undefined);
    const onGenerationProviderChange = vi.fn().mockResolvedValue(undefined);
    render(
      <KnowledgeHome
        topics={topics}
        providers={providers}
        generationProviderId="deepseek"
        availableCardCount={3}
        maxGenerationCount={10}
        busy={false}
        pendingGeneration={null}
        onBrowse={onBrowse}
        onGenerate={onGenerate}
        onGenerateRandom={onGenerateRandom}
        onContinueGeneration={vi.fn().mockResolvedValue(undefined)}
        onOpenModelSettings={vi.fn()}
        onGenerationProviderChange={onGenerationProviderChange}
      />,
    );

    const generationCount = screen.getByRole("spinbutton", {
      name: "本次生成知识卡数量",
    });
    expect(screen.getByRole("combobox", { name: "选择生成模型" })).toHaveValue(
      "deepseek",
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "选择生成模型" }),
      "kimi",
    );
    expect(onGenerationProviderChange).toHaveBeenCalledWith("kimi");
    await user.clear(generationCount);
    await user.type(generationCount, "3");
    const browseButton = screen.getByRole("button", {
      name: /浏览现有知识点/,
    });
    const randomButton = screen.getByRole("button", {
      name: /按兴趣权重随机生成/,
    });
    const topicButton = screen.getByRole("button", {
      name: /生成历史与文明知识点/,
    });
    expect(randomButton.closest(".generation-action-row")).toBe(
      topicButton.closest(".generation-action-row"),
    );
    expect(browseButton.closest(".feed-empty-card")?.lastElementChild).toBe(
      browseButton,
    );
    await user.click(browseButton);
    expect(onBrowse).toHaveBeenCalledOnce();
    await user.click(randomButton);
    expect(onGenerateRandom).toHaveBeenCalledWith(3);
    expect(topicButton).toBeVisible();
    await user.selectOptions(
      screen.getByRole("combobox", { name: "选择知识领域" }),
      "__custom__",
    );
    await user.type(
      screen.getByRole("textbox", { name: "自定义知识领域" }),
      "建筑与城市",
    );
    await user.click(
      screen.getByRole("button", { name: /生成建筑与城市知识点/ }),
    );
    expect(onGenerate).toHaveBeenCalledWith(null, "建筑与城市", 3);
  });

  it("offers an explicit continuation for a partially completed batch", async () => {
    const user = userEvent.setup();
    const onContinueGeneration = vi.fn().mockResolvedValue(undefined);
    render(
      <KnowledgeHome
        topics={topics}
        providers={providers}
        generationProviderId="deepseek"
        availableCardCount={3}
        maxGenerationCount={10}
        busy={false}
        pendingGeneration={{
          topicLabel: "历史与文明",
          completed: 4,
          total: 6,
          remaining: 2,
        }}
        onBrowse={vi.fn().mockResolvedValue(undefined)}
        onGenerate={vi.fn().mockResolvedValue(undefined)}
        onGenerateRandom={vi.fn().mockResolvedValue(undefined)}
        onContinueGeneration={onContinueGeneration}
        onOpenModelSettings={vi.fn()}
        onGenerationProviderChange={vi.fn().mockResolvedValue(undefined)}
      />,
    );

    expect(screen.getByText("已生成 4/6 条")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "继续生成剩余 2 条" }));
    expect(onContinueGeneration).toHaveBeenCalledOnce();
  });

  it("opens model settings for the selected provider when it is not ready", async () => {
    const user = userEvent.setup();
    const onOpenModelSettings = vi.fn();
    const commonProps = {
      topics,
      availableCardCount: 3,
      maxGenerationCount: 10,
      pendingGeneration: null,
      onBrowse: vi.fn().mockResolvedValue(undefined),
      onGenerate: vi.fn().mockResolvedValue(undefined),
      onGenerateRandom: vi.fn().mockResolvedValue(undefined),
      onContinueGeneration: vi.fn().mockResolvedValue(undefined),
      onOpenModelSettings,
      onGenerationProviderChange: vi.fn().mockResolvedValue(undefined),
    };
    const { rerender } = render(
      <KnowledgeHome
        {...commonProps}
        providers={providers}
        generationProviderId="kimi"
        busy={false}
      />,
    );

    expect(screen.getByRole("combobox", { name: "选择生成模型" })).toHaveValue(
      "kimi",
    );
    const configure = screen.getByRole("button", { name: /去配置/ });
    expect(configure).toBeEnabled();
    await user.click(configure);
    expect(onOpenModelSettings).toHaveBeenCalledOnce();

    rerender(
      <KnowledgeHome
        {...commonProps}
        providers={providers}
        generationProviderId="kimi"
        busy={true}
      />,
    );
    expect(screen.getByRole("button", { name: /去配置/ })).toBeDisabled();
    expect(
      screen.getByRole("combobox", { name: "选择生成模型" }),
    ).toBeDisabled();
    await user.click(screen.getByRole("button", { name: /去配置/ }));
    expect(onOpenModelSettings).toHaveBeenCalledOnce();

    const needsVerification = providers.map((provider) =>
      provider.id === "kimi"
        ? { ...provider, keyConfigured: true, keyLast4: "5678" }
        : provider,
    );
    rerender(
      <KnowledgeHome
        {...commonProps}
        providers={needsVerification}
        generationProviderId="kimi"
        busy={false}
      />,
    );
    expect(screen.getByRole("button", { name: /去测试/ })).toBeEnabled();

    rerender(
      <KnowledgeHome
        {...commonProps}
        providers={providers}
        generationProviderId="deepseek"
        busy={false}
      />,
    );
    expect(
      screen.queryByRole("button", { name: /去配置|去测试/ }),
    ).not.toBeInTheDocument();
  });
});
