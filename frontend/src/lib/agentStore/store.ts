import { create } from "zustand";
import { DEFAULT_OPERATOR_PROFILE_STATE } from "./operatorProfile";
import { createOperatorProfileAsyncActions } from "./operatorProfileAsyncActions";
import { createOperatorProfileStateActions } from "./operatorProfileStateActions";
import { createProviderActions } from "./providerActions";
import { createSettingsActions, createConciergeActions, createGatewayActions, DEFAULT_CONCIERGE_CONFIG } from "./settingsActions";
import { loadAgentSettings } from "./settings";
import type { AgentState } from "./storeTypes";
import { createThreadActions } from "./threadActions";

export const useAgentStore = create<AgentState>((set, get) => ({
  threads: [],
  messages: {},
  todos: {},
  activeThreadId: null,
  threadHistoryStack: [],
  agentPanelOpen: false,
  agentSettings: loadAgentSettings(),
  agentSettingsHydrated: false,
  agentSettingsDirty: false,
  searchQuery: "",
  providerAuthStates: [],
  subAgents: [],
  conciergeConfig: DEFAULT_CONCIERGE_CONFIG,
  coderConfig: {},
  conciergeWelcome: null,
  operatorProfile: DEFAULT_OPERATOR_PROFILE_STATE,
  gatewayStatuses: {},
  ...createProviderActions(set, get),
  ...createThreadActions(set, get),
  ...createSettingsActions(set, get),
  ...createConciergeActions(set, get),
  ...createOperatorProfileStateActions(set),
  ...createOperatorProfileAsyncActions(set, get),
  ...createGatewayActions(set),
}));
