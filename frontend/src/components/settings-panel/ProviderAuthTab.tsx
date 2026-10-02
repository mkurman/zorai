import { useEffect, useState } from "react";
import { getDaemonOwnedAuthCapability } from "@/lib/agentDaemonConfig";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "../../lib/agentStore";
import { deriveOpenAICodexAuthUi } from "./openaiSubscriptionAuth";
import { resolveOpenAIProviderRowState } from "./providerAuthRowState";
import { Section, inputStyle, smallBtnStyle } from "./shared";

export function ProviderAuthTab() {
    const agentBackend = useAgentStore((s) => s.agentSettings.agent_backend);
    const agentSettings = useAgentStore((s) => s.agentSettings);
    const providerAuthStates = useAgentStore((s) => s.providerAuthStates);
    const refreshProviderAuthStates = useAgentStore((s) => s.refreshProviderAuthStates);
    const validateProvider = useAgentStore((s) => s.validateProvider);
    const loginProvider = useAgentStore((s) => s.loginProvider);
    const logoutProvider = useAgentStore((s) => s.logoutProvider);

    const [filter, setFilter] = useState("");
    const [loginTarget, setLoginTarget] = useState<string | null>(null);
    const [loginKey, setLoginKey] = useState("");
    const [validating, setValidating] = useState<string | null>(null);
    const [cursorLoginNote, setCursorLoginNote] = useState<string | null>(null);
    const [validationResult, setValidationResult] = useState<Record<string, { valid: boolean; error?: string }>>({});

    // ChatGPT subscription auth state
    const [chatgptAuthStatus, setChatgptAuthStatus] = useState<ZoraiOpenAICodexAuthLogin | ZoraiOpenAICodexAuthStatus | null>(null);
    const [chatgptAuthBusy, setChatgptAuthBusy] = useState(false);
    const chatgptAuthUi = deriveOpenAICodexAuthUi(chatgptAuthStatus);
    const authCapability = getDaemonOwnedAuthCapability(agentBackend);

    const openChatgptAuthUrl = (url: string) => {
        if (!url) return;
        const opened = typeof window !== "undefined"
            ? window.open(url, "_blank", "noopener,noreferrer")
            : null;
        if (opened) {
            opened.opener = null;
        }
    };

    useEffect(() => {
        refreshProviderAuthStates();
    }, [refreshProviderAuthStates]);

    useEffect(() => {
        if (!authCapability.chatgptSubscriptionAvailable) {
            setChatgptAuthStatus(null);
            return;
        }

        const zorai = getBridge();
        if (!zorai?.openAICodexAuthStatus) {
            return;
        }

        let cancelled = false;
        void zorai.openAICodexAuthStatus({ refresh: true }).then((status: any) => {
            if (cancelled) {
                return;
            }
            setChatgptAuthStatus(status);
        }).catch(() => {});

        return () => {
            cancelled = true;
        };
    }, [authCapability.chatgptSubscriptionAvailable]);

    // Poll ChatGPT auth status when auth URL is active
    useEffect(() => {
        if (!chatgptAuthUi.shouldPoll) return;
        const timer = window.setInterval(() => {
            const zorai = getBridge();
            if (!zorai?.openAICodexAuthStatus) return;
            void zorai.openAICodexAuthStatus({ refresh: true }).then((status: any) => {
                const nextUi = deriveOpenAICodexAuthUi(status);
                setChatgptAuthStatus(status);
                if (nextUi.isTerminal) {
                    refreshProviderAuthStates();
                }
            }).catch(() => {});
        }, 2000);
        return () => window.clearInterval(timer);
    }, [chatgptAuthUi.shouldPoll, refreshProviderAuthStates]);

    const filtered = providerAuthStates.filter((s) =>
        !filter || s.provider_name.toLowerCase().includes(filter.toLowerCase()) || s.provider_id.toLowerCase().includes(filter.toLowerCase())
    );

    const handleLogin = async (providerId: string) => {
        if (!loginKey.trim()) return;
        const state = filtered.find((s) => s.provider_id === providerId);
        await loginProvider(providerId, loginKey, state?.base_url);
        setLoginKey("");
        setLoginTarget(null);
    };

    const handleLogout = async (providerId: string) => {
        await logoutProvider(providerId);
    };

    const handleChatgptLogin = async () => {
        if (!authCapability.chatgptSubscriptionAvailable) {
            setChatgptAuthStatus({ available: false, authMode: "chatgpt_subscription", status: "error", error: "ChatGPT subscription auth requires daemon-backed execution" } as ZoraiOpenAICodexAuthStatus);
            return;
        }
        const zorai = getBridge();
        if (!zorai?.openAICodexAuthLogin) return;
        setChatgptAuthBusy(true);
        try {
            const result = await zorai.openAICodexAuthLogin();
            const authUrl = deriveOpenAICodexAuthUi(result).authUrl;
            setChatgptAuthStatus(result);
            if (authUrl) {
                openChatgptAuthUrl(authUrl);
            }
            if (deriveOpenAICodexAuthUi(result).isTerminal) {
                refreshProviderAuthStates();
            }
        } catch (error: any) {
            void error;
        } finally {
            setChatgptAuthBusy(false);
        }
    };

    const handleChatgptLogout = async () => {
        if (!authCapability.chatgptSubscriptionAvailable) {
            setChatgptAuthStatus({ available: false, authMode: "chatgpt_subscription", status: "error", error: "ChatGPT subscription auth requires daemon-backed execution" } as ZoraiOpenAICodexAuthStatus);
            return;
        }
        const zorai = getBridge();
        if (!zorai?.openAICodexAuthLogout) return;
        setChatgptAuthBusy(true);
        try {
            await zorai.openAICodexAuthLogout();
            setChatgptAuthStatus({ available: false, authMode: "chatgpt_subscription" });
            refreshProviderAuthStates();
        } catch { /* ignore */ } finally {
            setChatgptAuthBusy(false);
        }
    };

    const handleTest = async (providerId: string) => {
        setValidating(providerId);
        const state = filtered.find((s) => s.provider_id === providerId);
        const usesChatgptSubscription = providerId === "openai" && state?.auth_source === "chatgpt_subscription";
        try {
            if (usesChatgptSubscription) {
                if (!authCapability.chatgptSubscriptionAvailable) {
                    setValidationResult((prev) => ({
                        ...prev,
                        [providerId]: { valid: false, error: "ChatGPT subscription auth requires daemon-backed execution" },
                    }));
                    return;
                }
                const zorai = getBridge();
                if (!zorai?.openAICodexAuthStatus) {
                    setValidationResult((prev) => ({
                        ...prev,
                        [providerId]: { valid: false, error: "ChatGPT auth bridge unavailable" },
                    }));
                    return;
                }
                const status = await zorai.openAICodexAuthStatus({ refresh: true });
                setChatgptAuthStatus(status);
                setValidationResult((prev) => ({
                    ...prev,
                    [providerId]: status?.available
                        ? { valid: true }
                        : { valid: false, error: status?.error || "ChatGPT subscription auth not available" },
                }));
                return;
            }

            const result = await validateProvider(
                providerId,
                state?.base_url || "",
                "",
                state?.auth_source || "api_key"
            );
            setValidationResult((prev) => ({ ...prev, [providerId]: result }));
        } finally {
            setValidating(null);
        }
    };

    return (
        <div>
            <Section title="Provider Authentication">
                <div style={{ marginBottom: 12 }}>
                    <input
                        type="text"
                        placeholder="Filter providers..."
                        value={filter}
                        onChange={(e) => setFilter(e.target.value)}
                        style={{ ...inputStyle, width: "100%" }}
                    />
                </div>
                {providerAuthStates.length === 0 && (
                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", padding: "8px 0" }}>
                        Loading providers from daemon...
                    </div>
                )}
                <div style={{ display: "grid", gap: 2 }}>
                    {filtered.map((state) => {
                        const isExpanded = loginTarget === state.provider_id;
                        const vr = validationResult[state.provider_id];
                        const isOpenAI = state.provider_id === "openai";
                        const isGithubCopilot = state.provider_id === "github-copilot";
                        const isCursor = state.provider_id === "cursor";
                        const canUseChatgptSubscription = isOpenAI && authCapability.chatgptSubscriptionAvailable;
                        const configuredOpenAIAuthSource = isOpenAI
                            ? agentSettings.openai?.auth_source ?? state.auth_source
                            : state.auth_source;
                        const rowState = resolveOpenAIProviderRowState({
                            providerId: state.provider_id,
                            providerAuthenticated: state.authenticated,
                            providerAuthSource: state.auth_source as any,
                            selectedAuthSource: configuredOpenAIAuthSource as any,
                            chatgptAvailable: chatgptAuthStatus?.available === true,
                        });
                        const isOpenAIApiKeyExpanded = isExpanded && rowState.showApiKeyLogin;
                        const authButtonLabel = isGithubCopilot ? "Token" : "API Key";
                        const keyPlaceholder = isOpenAI
                            ? "OpenAI API Key"
                            : isGithubCopilot
                                ? "GitHub Copilot Token"
                                : "API Key";
                        return (
                            <div key={state.provider_id} style={{
                                border: "1px solid var(--border)",
                                background: "var(--zorai-bg-panel)",
                                padding: "8px 12px",
                            }}>
                                <div style={{
                                    display: "flex",
                                    alignItems: "center",
                                    justifyContent: "space-between",
                                    gap: 8,
                                }}>
                                    <div style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
                                        <span style={{
                                            width: 8,
                                            height: 8,
                                            borderRadius: "50%",
                                            background: rowState.authenticated ? "#4ade80" : "#6b7280",
                                            flexShrink: 0,
                                        }} />
                                        <span style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>{state.provider_name}</span>
                                        {rowState.authenticated && (
                                            <span style={{
                                                fontSize: "var(--text-sm)",
                                                color: "var(--text-secondary)",
                                                background: "var(--zorai-bg-panel)",
                                                padding: "1px 6px",
                                                borderRadius: 3,
                                            }}>
                                                {state.model || "—"}
                                            </span>
                                        )}
                                    </div>
                                    <div style={{ display: "flex", gap: 4 }}>
                                        {isOpenAI ? (
                                            <>
                                                {rowState.showApiKeyLogin ? (
                                                    <button
                                                        onClick={() => {
                                                            setLoginTarget(isOpenAIApiKeyExpanded ? null : state.provider_id);
                                                            setLoginKey("");
                                                        }}
                                                        style={{ ...smallBtnStyle, fontSize: "var(--text-sm)" }}
                                                    >
                                                        {isOpenAIApiKeyExpanded ? "Cancel" : authButtonLabel}
                                                    </button>
                                                ) : rowState.showApiKeyLogout ? (
                                                    <button
                                                        onClick={() => handleLogout(state.provider_id)}
                                                        style={{ ...smallBtnStyle, fontSize: "var(--text-sm)", color: "#ef4444" }}
                                                    >
                                                        Logout
                                                    </button>
                                                ) : null}
                                                {canUseChatgptSubscription && rowState.showChatgptLogin ? (
                                                    <button
                                                        onClick={handleChatgptLogin}
                                                        disabled={chatgptAuthBusy}
                                                        style={{ ...smallBtnStyle, fontSize: "var(--text-sm)", color: "var(--accent)" }}
                                                    >
                                                        {chatgptAuthBusy ? "..." : "ChatGPT Login"}
                                                    </button>
                                                ) : null}
                                                {canUseChatgptSubscription && rowState.showChatgptLogout ? (
                                                    <button
                                                        onClick={handleChatgptLogout}
                                                        style={{ ...smallBtnStyle, fontSize: "var(--text-sm)", color: "#ef4444" }}
                                                    >
                                                        Logout
                                                    </button>
                                                ) : null}
                                                <button
                                                    onClick={() => handleTest(state.provider_id)}
                                                    disabled={validating === state.provider_id}
                                                    style={{ ...smallBtnStyle, fontSize: "var(--text-sm)" }}
                                                >
                                                    {validating === state.provider_id ? "Testing..." : "Test"}
                                                </button>
                                            </>
                                        ) : isCursor ? (
                                            <>
                                                {state.authenticated ? (
                                                    <button
                                                        onClick={() => handleLogout(state.provider_id)}
                                                        style={{ ...smallBtnStyle, fontSize: "var(--text-sm)", color: "#ef4444" }}
                                                    >
                                                        Logout
                                                    </button>
                                                ) : (
                                                    <button
                                                        onClick={() => {
                                                            setCursorLoginNote("Finish Cursor login in the browser, then refresh auth status.");
                                                            void loginProvider(state.provider_id, "", state.base_url);
                                                        }}
                                                        style={{ ...smallBtnStyle, fontSize: "var(--text-sm)", color: "var(--accent)" }}
                                                    >
                                                        Subscription
                                                    </button>
                                                )}
                                                <button
                                                    onClick={() => handleTest(state.provider_id)}
                                                    disabled={validating === state.provider_id}
                                                    style={{ ...smallBtnStyle, fontSize: "var(--text-sm)" }}
                                                >
                                                    {validating === state.provider_id ? "Testing..." : "Test"}
                                                </button>
                                            </>
                                        ) : (
                                            <>
                                                {rowState.showApiKeyLogin && (
                                                    <button
                                                        onClick={() => {
                                                            setLoginTarget(isExpanded ? null : state.provider_id);
                                                            setLoginKey("");
                                                        }}
                                                        style={{ ...smallBtnStyle, fontSize: "var(--text-sm)" }}
                                                    >
                                                        {isExpanded ? "Cancel" : authButtonLabel}
                                                    </button>
                                                )}
                                                <button
                                                    onClick={() => handleTest(state.provider_id)}
                                                    disabled={validating === state.provider_id}
                                                    style={{ ...smallBtnStyle, fontSize: "var(--text-sm)" }}
                                                >
                                                    {validating === state.provider_id ? "Testing..." : "Test"}
                                                </button>
                                            </>
                                        )}
                                        </div>
                                </div>
                                {vr && (
                                    <div style={{
                                        fontSize: "var(--text-sm)",
                                        marginTop: 4,
                                        color: vr.valid ? "#4ade80" : "#ef4444",
                                    }}>
                                        {vr.valid ? "Connection OK" : `Error: ${vr.error || "unknown"}`}
                                    </div>
                                )}
                                {isCursor && cursorLoginNote && !state.authenticated ? (
                                    <div style={{ fontSize: "var(--text-sm)", marginTop: 4, color: "var(--text-secondary)" }}>
                                        {cursorLoginNote}
                                    </div>
                                ) : null}
                                {isOpenAI && !authCapability.chatgptSubscriptionAvailable && (
                                    <div style={{ fontSize: "var(--text-sm)", marginTop: 4, color: "var(--text-secondary)" }}>
                                        ChatGPT Subscription is unavailable for the current backend. Switch to daemon-backed execution to enable it.
                                    </div>
                                )}
                                {isOpenAI && chatgptAuthUi.authUrl && (
                                    <div style={{ fontSize: "var(--text-sm)", marginTop: 4, color: "var(--accent)" }}>
                                        <span>Auth URL: </span>
                                        <a
                                            href={chatgptAuthUi.authUrl}
                                            target="_blank"
                                            rel="noreferrer"
                                            onClick={(event) => {
                                                event.preventDefault();
                                                openChatgptAuthUrl(chatgptAuthUi.authUrl!);
                                            }}
                                            style={{ color: "var(--accent)", textDecoration: "underline" }}
                                        >
                                            Open in browser
                                        </a>
                                        <span style={{ marginLeft: 8, color: "var(--text-secondary)" }}>Waiting for confirmation...</span>
                                    </div>
                                )}
                                {isOpenAIApiKeyExpanded || (isExpanded && !isOpenAI && rowState.showApiKeyLogin) ? (
                                    <div style={{
                                        display: "flex",
                                        gap: 6,
                                        marginTop: 8,
                                        alignItems: "center",
                                    }}>
                                        <input
                                            type="password"
                                            placeholder={keyPlaceholder}
                                            value={loginKey}
                                            onChange={(e) => setLoginKey(e.target.value)}
                                            onKeyDown={(e) => {
                                                if (e.key === "Enter") handleLogin(state.provider_id);
                                            }}
                                            style={{ ...inputStyle, flex: 1 }}
                                            autoFocus
                                        />
                                        <button
                                            onClick={() => handleLogin(state.provider_id)}
                                            disabled={!loginKey.trim()}
                                            style={smallBtnStyle}
                                        >
                                            Save
                                        </button>
                                    </div>
                                ) : null}
                            </div>
                        );
                    })}
                </div>
            </Section>
        </div>
    );
}
