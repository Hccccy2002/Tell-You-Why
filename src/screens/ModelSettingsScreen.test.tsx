import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";
import {
  CREDENTIAL_REPLACEMENT_REQUIRED,
  bootstrapApp,
  deleteProviderKey,
  saveProviderProfile,
} from "../lib/api";
import type { ProviderSpec } from "../types";
import { ModelSettingsScreen } from "./ModelSettingsScreen";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal();
  if (!actual || typeof actual !== "object") {
    throw new Error("Failed to load the real API module for this test");
  }
  return {
    ...actual,
    deleteProviderKey: vi.fn().mockResolvedValue(undefined),
    saveProviderProfile: vi.fn(),
  };
});

const deleteProviderKeyMock = vi.mocked(deleteProviderKey);
const saveProviderProfileMock = vi.mocked(saveProviderProfile);

function withConfiguredDeepSeek(providers: ProviderSpec[]): ProviderSpec[] {
  return providers.map((provider) =>
    provider.id === "deepseek"
      ? {
          ...provider,
          keyConfigured: true,
          keyLast4: "1234",
          connectionVerified: true,
        }
      : provider,
  );
}

describe("ModelSettingsScreen", () => {
  it("places Zhipu search alongside DeepSeek and Kimi without changing the generation provider", async () => {
    const bootstrap = await bootstrapApp();
    const changed = vi.fn();
    const user = userEvent.setup();
    render(
      <ModelSettingsScreen
        initialProviders={bootstrap.providers}
        onProvidersChanged={changed}
      />,
    );
    expect(screen.getByRole("tab", { name: "DeepSeek" })).toBeVisible();
    expect(screen.getByRole("tab", { name: "Kimi" })).toBeVisible();
    await user.click(screen.getByRole("tab", { name: "智谱搜索" }));
    const input = await screen.findByLabelText("智谱 API Key");
    await waitFor(() => expect(input).toBeEnabled());
    expect(screen.getByRole("tab", { name: "DeepSeek" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
    expect(changed).not.toHaveBeenCalled();
    await user.click(screen.getByRole("tab", { name: "DeepSeek" }));
    expect(screen.getByLabelText("API Key")).toBeVisible();
  });
  beforeEach(() => {
    deleteProviderKeyMock.mockReset();
    deleteProviderKeyMock.mockResolvedValue(undefined);
    saveProviderProfileMock.mockReset();
  });

  it("keeps generation user-triggered and does not expose auto replenishment", async () => {
    const bootstrap = await bootstrapApp();
    await act(async () => {
      render(
        <ModelSettingsScreen
          initialProviders={bootstrap.providers}
          onProvidersChanged={vi.fn()}
        />,
      );
      await Promise.resolve();
    });

    expect(screen.queryByText("自动补充内容")).not.toBeInTheDocument();
    expect(screen.queryByText("立即补充 5 张")).not.toBeInTheDocument();
  });

  it("deletes a configured API Key only after cancelling and confirming the reopened dialog", async () => {
    const bootstrap = await bootstrapApp();
    const providers = withConfiguredDeepSeek(bootstrap.providers).map(
      (provider) =>
        provider.id === "deepseek"
          ? {
              ...provider,
              selectedModel:
                provider.models.find((model) => !model.recommended)?.id ??
                provider.selectedModel,
            }
          : provider,
    );
    const onProvidersChanged = vi.fn();
    const user = userEvent.setup();
    render(
      <ModelSettingsScreen
        initialProviders={providers}
        onProvidersChanged={onProvidersChanged}
      />,
    );

    const deleteButton = screen.getByRole("button", {
      name: "删除 DeepSeek API Key",
    });
    await user.click(deleteButton);
    const firstDialog = screen.getByRole("dialog", {
      name: "确认删除 DeepSeek API Key 吗？",
    });

    expect(deleteProviderKeyMock).not.toHaveBeenCalled();
    expect(onProvidersChanged).not.toHaveBeenCalled();
    await user.click(within(firstDialog).getByRole("button", { name: "取消" }));
    expect(deleteProviderKeyMock).not.toHaveBeenCalled();
    expect(onProvidersChanged).not.toHaveBeenCalled();

    await user.click(deleteButton);
    const reopenedDialog = screen.getByRole("dialog", {
      name: "确认删除 DeepSeek API Key 吗？",
    });
    await user.click(
      within(reopenedDialog).getByRole("button", { name: "删除 API Key" }),
    );

    await waitFor(() => {
      expect(deleteProviderKeyMock).toHaveBeenCalledTimes(1);
      expect(onProvidersChanged).toHaveBeenCalledTimes(1);
    });
    expect(deleteProviderKeyMock).toHaveBeenCalledWith("deepseek", "default");
    expect(onProvidersChanged).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({
          id: "deepseek",
          keyConfigured: false,
          keyLast4: null,
          connectionVerified: false,
          selectedRegion: "default",
          selectedModel: "deepseek-v4-flash",
        }),
      ]),
    );
  });

  it("replaces an existing API Key only after cancelling and confirming the reopened dialog", async () => {
    const bootstrap = await bootstrapApp();
    const providers = withConfiguredDeepSeek(bootstrap.providers);
    const current = providers.find((provider) => provider.id === "deepseek");
    if (!current) throw new Error("expected DeepSeek provider");
    const savedProvider: ProviderSpec = {
      ...current,
      keyLast4: "5678",
      connectionVerified: false,
    };
    saveProviderProfileMock.mockResolvedValue(savedProvider);
    const onProvidersChanged = vi.fn();
    const user = userEvent.setup();
    render(
      <ModelSettingsScreen
        initialProviders={providers}
        onProvidersChanged={onProvidersChanged}
      />,
    );

    const keyInput = screen.getByLabelText("API Key");
    await user.type(keyInput, "sk-replacement-key");
    const saveButton = screen.getByRole("button", { name: "保存配置" });
    await user.click(saveButton);
    const firstDialog = screen.getByRole("dialog", {
      name: "确认覆盖 DeepSeek API Key 吗？",
    });

    expect(saveProviderProfileMock).not.toHaveBeenCalled();
    expect(onProvidersChanged).not.toHaveBeenCalled();
    await user.click(within(firstDialog).getByRole("button", { name: "取消" }));
    expect(keyInput).toHaveValue("sk-replacement-key");
    expect(saveProviderProfileMock).not.toHaveBeenCalled();
    expect(onProvidersChanged).not.toHaveBeenCalled();

    await user.click(saveButton);
    const reopenedDialog = screen.getByRole("dialog", {
      name: "确认覆盖 DeepSeek API Key 吗？",
    });
    await user.click(
      within(reopenedDialog).getByRole("button", {
        name: "确认覆盖 API Key",
      }),
    );

    await waitFor(() => {
      expect(saveProviderProfileMock).toHaveBeenCalledTimes(1);
      expect(onProvidersChanged).toHaveBeenCalledTimes(1);
    });
    expect(saveProviderProfileMock).toHaveBeenCalledWith({
      providerId: "deepseek",
      region: "default",
      model: current.selectedModel,
      apiKey: "sk-replacement-key",
      replaceExistingKey: true,
    });
    expect(keyInput).toHaveValue("");
  });

  it("honors a backend replacement challenge for a channel hidden by provider-level state", async () => {
    const bootstrap = await bootstrapApp();
    const current = bootstrap.providers.find(
      (provider) => provider.id === "deepseek",
    );
    if (!current) throw new Error("expected DeepSeek provider");
    let rejectInitialSave: ((reason?: unknown) => void) | undefined;
    saveProviderProfileMock
      .mockImplementationOnce(
        () =>
          new Promise((_resolve, reject) => {
            rejectInitialSave = reject;
          }),
      )
      .mockResolvedValueOnce({
        ...current,
        keyConfigured: true,
        keyLast4: "9876",
      });
    const onProvidersChanged = vi.fn();
    const user = userEvent.setup();
    render(
      <ModelSettingsScreen
        initialProviders={bootstrap.providers}
        onProvidersChanged={onProvidersChanged}
      />,
    );

    const keyInput = screen.getByLabelText("API Key");
    await user.type(keyInput, "sk-hidden-channel-replacement");
    await user.click(screen.getByRole("button", { name: "保存配置" }));

    await waitFor(() => {
      expect(saveProviderProfileMock).toHaveBeenCalledTimes(1);
    });
    expect(keyInput).toBeDisabled();
    expect(screen.getByLabelText("服务通道")).toBeDisabled();
    expect(screen.getByLabelText("模型")).toBeDisabled();
    expect(screen.getByRole("tab", { name: /Kimi/ })).toBeDisabled();

    act(() => {
      rejectInitialSave?.(new Error(CREDENTIAL_REPLACEMENT_REQUIRED));
    });

    const dialog = await screen.findByRole("dialog", {
      name: "确认覆盖 DeepSeek API Key 吗？",
    });
    expect(saveProviderProfileMock).toHaveBeenNthCalledWith(1, {
      providerId: "deepseek",
      region: "default",
      model: current.selectedModel,
      apiKey: "sk-hidden-channel-replacement",
      replaceExistingKey: false,
    });
    expect(keyInput).toHaveValue("sk-hidden-channel-replacement");
    expect(onProvidersChanged).not.toHaveBeenCalled();

    await user.click(
      within(dialog).getByRole("button", { name: "确认覆盖 API Key" }),
    );
    await waitFor(() => {
      expect(saveProviderProfileMock).toHaveBeenCalledTimes(2);
      expect(onProvidersChanged).toHaveBeenCalledTimes(1);
    });
    expect(saveProviderProfileMock).toHaveBeenNthCalledWith(2, {
      providerId: "deepseek",
      region: "default",
      model: current.selectedModel,
      apiKey: "sk-hidden-channel-replacement",
      replaceExistingKey: true,
    });
    expect(keyInput).toHaveValue("");
  });

  it("keeps a failed API Key deletion confirmation open and allows retry", async () => {
    deleteProviderKeyMock
      .mockRejectedValueOnce(new Error("系统凭据库暂时不可用"))
      .mockResolvedValueOnce(undefined);
    const bootstrap = await bootstrapApp();
    const providers = withConfiguredDeepSeek(bootstrap.providers);
    const onProvidersChanged = vi.fn();
    const user = userEvent.setup();
    render(
      <ModelSettingsScreen
        initialProviders={providers}
        onProvidersChanged={onProvidersChanged}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: "删除 DeepSeek API Key" }),
    );
    const dialog = screen.getByRole("dialog", {
      name: "确认删除 DeepSeek API Key 吗？",
    });
    const confirm = within(dialog).getByRole("button", {
      name: "删除 API Key",
    });
    await user.click(confirm);

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "系统凭据库暂时不可用",
    );
    expect(deleteProviderKeyMock).toHaveBeenCalledTimes(1);
    expect(onProvidersChanged).not.toHaveBeenCalled();

    await user.click(confirm);
    await waitFor(() => {
      expect(deleteProviderKeyMock).toHaveBeenCalledTimes(2);
      expect(onProvidersChanged).toHaveBeenCalledTimes(1);
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
