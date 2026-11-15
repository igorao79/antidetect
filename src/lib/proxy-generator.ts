import { invoke } from "@tauri-apps/api/core";
import type { StoredProxy } from "@/types";
import type {
  GenerateProxiesRequest,
  ProxyGeneratorOptions,
} from "./proxy-generator-form";

/** Whether this device holds a generator key pair (fork). */
export interface ProxyGeneratorStatus {
  configured: boolean;
  key_hint: string | null;
}

export const getProxyGeneratorStatus = () =>
  invoke<ProxyGeneratorStatus>("proxy_generator_status");

export const setProxyGeneratorCredentials = (key: string, secret: string) =>
  invoke<ProxyGeneratorStatus>("proxy_generator_set_credentials", {
    key,
    secret,
  });

export const clearProxyGeneratorCredentials = () =>
  invoke<void>("proxy_generator_clear_credentials");

export const getProxyGeneratorOptions = () =>
  invoke<ProxyGeneratorOptions>("proxy_generator_options");

export const generateProxies = (request: GenerateProxiesRequest) =>
  invoke<StoredProxy[]>("proxy_generator_generate", { request });

/** Emitted when erasing a generated proxy could not release its port. */
export const RELEASE_FAILED_EVENT = "proxy-generator-release-failed";
