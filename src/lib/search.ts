import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./api";

export interface SearchSettings {
  keyConfigured: boolean;
  keyLast4: string | null;
  connectionVerified: boolean;
  options?: SearchOptions;
  attemptsToday?: number;
}

export interface SearchOptions {
  mode: "off" | "auto" | "always";
  engine: "search_pro" | "search_std";
  dailyAttemptLimit: number;
}
export const defaultSearchOptions: SearchOptions = {
  mode: "off",
  engine: "search_pro",
  dailyAttemptLimit: 50,
};
export async function saveSearchOptions(
  options: SearchOptions,
): Promise<SearchSettings> {
  requireDesktop();
  return invoke<SearchSettings>("save_search_options", { options });
}

export async function getSearchSettings(): Promise<SearchSettings> {
  if (!isDesktop())
    return { keyConfigured: false, keyLast4: null, connectionVerified: false };
  return invoke<SearchSettings>("search_settings");
}

function requireDesktop() {
  if (!isDesktop())
    throw new Error(
      "请在桌面 APP 中配置智谱 API Key，浏览器预览不会保存凭据。",
    );
}

export async function saveSearchKey(
  apiKey: string,
  replaceExistingKey: boolean,
): Promise<SearchSettings> {
  requireDesktop();
  return invoke<SearchSettings>("save_search_key", {
    input: { apiKey, replaceExistingKey },
  });
}

export async function deleteSearchKey(): Promise<void> {
  requireDesktop();
  return invoke("delete_search_key");
}

export async function testSearchConnection(): Promise<SearchSettings> {
  requireDesktop();
  return invoke<SearchSettings>("test_search_connection");
}

export async function prepareSearchFollowUp(
  cardId: string,
  force: boolean,
): Promise<string> {
  requireDesktop();
  return invoke("prepare_search_follow_up", { cardId, force });
}
export async function searchFollowUpStatus(
  cardId: string,
  runId: string,
): Promise<string | null> {
  return invoke("search_follow_up_status", { cardId, runId });
}
export async function cancelSearchFollowUp(
  cardId: string,
  runId: string,
): Promise<boolean> {
  return invoke("cancel_search_follow_up", { cardId, runId });
}
