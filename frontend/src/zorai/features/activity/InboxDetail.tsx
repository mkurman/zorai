import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { MarkdownContent } from "@/components/agent-chat-panel/chat-view/markdown";
import { resolveApprovalAtSource, type ApprovalDecisionStatus } from "@/components/approvalResolution";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useAgentStore } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import { useNotificationStore } from "@/lib/notificationStore";
import { useOperatorQuestionStore } from "@/lib/operatorQuestionStore";
import { formatWhen, notificationTime, type InboxItem } from "./inboxModel";
import { useInboxStore } from "./inboxStore";
import { ThreadComposer } from "../threads/ThreadComposer";
import { readComposerDraftInput, writeComposerDraftInput } from "../threads/composerDraftStore";

type DetailProps = {
  item: InboxItem;
  onOpenThread: (threadId: string) => void;
  onOpenGoal: (goalRunId: string) => void;
};

type ThreadMessage = { role?: string; content?: string; created_at?: number; timestamp?: number };

export function InboxDetail({ item, onOpenThread, onOpenGoal }: DetailProps) {
  const links = (
    <div className="zorai-inbox-detail__links">
      {item.threadId && !item.threadId.startsWith("goal:") ? (
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => onOpenThread(item.threadId!)}>Open thread</button>
      ) : null}
      {item.goalRunId ? (
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => onOpenGoal(item.goalRunId!)}>Open goal</button>
      ) : null}
    </div>
  );

  return (
    <article className="zorai-inbox-detail" aria-label={item.title}>
      <header className="zorai-inbox-detail__header">
        <div className="zorai-inbox-detail__kicker">{detailKicker(item)} · {formatWhen(item.at)}</div>
        <h2>{item.title}</h2>
        {links}
      </header>
      <div className="zorai-inbox-detail__body">
        {item.kind === "approval" ? <ApprovalDetail item={item} /> : null}
        {item.kind === "question" ? <QuestionDetail item={item} /> : null}
        {item.kind === "goal" ? <p>{item.summary}</p> : null}
        {item.kind === "reply" ? <ReplyDetail key={item.threadKey} item={item} /> : null}
        {item.kind === "notification" ? <NotificationDetail item={item} /> : null}
        {item.kind === "updates" ? <UpdatesDetail item={item} /> : null}
      </div>
    </article>
  );
}

function ApprovalDetail({ item }: { item: Extract<InboxItem, { kind: "approval" }> }) {
  const resolveApproval = useAgentMissionStore((state) => state.resolveApproval);
  const markApprovalHandled = useAgentMissionStore((state) => state.markApprovalHandled);
  const [busy, setBusy] = useState(false);
  const approval = item.approval;

  const decide = async (status: ApprovalDecisionStatus) => {
    if (busy) return;
    setBusy(true);
    try {
      const resolved = await resolveApprovalAtSource(getBridge(), approval, status);
      if (!resolved) return;
      resolveApproval(approval.id, status);
      if (approval.source !== "local-terminal") markApprovalHandled(approval.id);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <pre className="zorai-inbox-detail__command">{approval.command}</pre>
      <dl className="zorai-inbox-detail__facts">
        <dt>Risk</dt><dd>{approval.riskLevel}</dd>
        {approval.blastRadius ? <><dt>Blast radius</dt><dd>{approval.blastRadius}</dd></> : null}
        {approval.reasons.length > 0 ? <><dt>Why</dt><dd>{approval.reasons.join("; ")}</dd></> : null}
      </dl>
      <div className="zorai-inbox-detail__actions">
        <button type="button" className="zorai-primary-button" disabled={busy} onClick={() => void decide("approved-once")}>Approve once</button>
        <button type="button" className="zorai-ghost-button" disabled={busy} onClick={() => void decide("approved-session")}>Approve for session</button>
        <button type="button" className="zorai-ghost-button zorai-inbox-detail__deny" disabled={busy} onClick={() => void decide("denied")}>Deny</button>
      </div>
    </>
  );
}

function QuestionDetail({ item }: { item: Extract<InboxItem, { kind: "question" }> }) {
  const resolveQuestion = useOperatorQuestionStore((state) => state.resolveQuestion);
  const [answer, setAnswer] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (value: string) => {
    const trimmed = value.trim();
    if (busy || !trimmed) return;
    setBusy(true);
    setError(null);
    try {
      const result = await getBridge()?.agentAnswerQuestion?.(item.question.question_id, trimmed) as { ok?: boolean; error?: string } | undefined;
      if (result && result.ok === false) {
        setError(result.error || "Failed to send the answer.");
        return;
      }
      resolveQuestion(item.question.question_id);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="zorai-inbox-detail__message"><MarkdownContent content={item.question.content} /></div>
      {item.question.options.length > 0 ? (
        <div className="zorai-inbox-detail__actions">
          {item.question.options.map((option) => (
            <button key={option} type="button" className="zorai-ghost-button" disabled={busy} onClick={() => void submit(option)}>{option}</button>
          ))}
        </div>
      ) : null}
      <Composer value={answer} onChange={setAnswer} onSubmit={() => void submit(answer)} busy={busy} placeholder="Type your own answer… (Enter sends)" />
      {error ? <div className="zorai-inbox-detail__error" role="alert">{error}</div> : null}
    </>
  );
}

function canReplyTo(threadId: string | null): threadId is string {
  return Boolean(threadId) && !threadId!.startsWith("goal:");
}

function useMarkOnceOnOpen(mark: () => void) {
  const markRef = useRef(mark);
  markRef.current = mark;
  useEffect(() => {
    markRef.current();
  }, []);
}

function ThreadConversation({ threadKey, agentName, title, refreshKey }: { threadKey: string; agentName: string; title: string; refreshKey: number }) {
  const markThreadRead = useInboxStore((state) => state.markThreadRead);
  const [messages, setMessages] = useState<ThreadMessage[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const [sent, setSent] = useState(false);

  useEffect(() => {
    const savedDraft = readComposerDraftInput();
    writeComposerDraftInput("");
    return () => writeComposerDraftInput(savedDraft);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoadError(null);
    const bridge = getBridge();
    if (!bridge?.agentGetThread) {
      setMessages([]);
      return;
    }
    void bridge.agentGetThread(threadKey, { messageLimit: 6 })
      .then((thread) => {
        if (cancelled) return;
        const loaded = ((thread as { messages?: ThreadMessage[] } | null)?.messages ?? [])
          .filter((message) => (message.role === "assistant" || message.role === "user") && message.content?.trim());
        setMessages(loaded.slice(-4));
      })
      .catch((reason: unknown) => {
        if (!cancelled) setLoadError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => { cancelled = true; };
  }, [refreshKey, threadKey]);

  const send = async (payload: { text: string; contentBlocksJson?: string | null }) => {
    setSendError(null);
    const result = await getBridge()?.agentSendMessage?.(threadKey, payload.text, null, undefined, payload.contentBlocksJson ?? null) as { ok?: boolean; error?: string } | undefined;
    if (result && result.ok === false) {
      setSendError(result.error || "Failed to send the reply.");
      return;
    }
    setSent(true);
    markThreadRead(threadKey, Date.now());
  };

  return (
    <section className="zorai-inbox-thread" aria-label="Thread conversation">
      <div className="zorai-inbox-detail__role">Latest in this thread</div>
      {loadError ? <div className="zorai-inbox-detail__error" role="alert">Could not load the thread: {loadError}</div> : null}
      {messages === null && !loadError ? <div className="zorai-inbox-detail__note">Loading the latest messages…</div> : null}
      {messages && messages.length === 0 ? <div className="zorai-inbox-detail__note">No messages yet.</div> : null}
      {messages?.map((message, index) => (
        <div key={index} className={`zorai-inbox-detail__message zorai-inbox-detail__message--${message.role}`}>
          <div className="zorai-inbox-detail__role">{message.role === "user" ? "You" : agentName || "Agent"}</div>
          <MarkdownContent content={message.content ?? ""} />
        </div>
      ))}
      {sent ? <div className="zorai-inbox-detail__note">Reply sent. The agent's answer will show up in the inbox when it finishes.</div> : null}
      <div className="zorai-inbox-thread__composer">
        <ThreadComposer placeholder={`Reply to ${title}…`} onSendNewThread={(payload) => void send(payload)} />
      </div>
      {sendError ? <div className="zorai-inbox-detail__error" role="alert">{sendError}</div> : null}
    </section>
  );
}

function ReplyDetail({ item }: { item: Extract<InboxItem, { kind: "reply" }> }) {
  const markThreadRead = useInboxStore((state) => state.markThreadRead);
  const markThreadUnread = useInboxStore((state) => state.markThreadUnread);
  const archiveThread = useInboxStore((state) => state.archiveThread);

  useMarkOnceOnOpen(() => markThreadRead(item.threadKey, item.at));

  return (
    <>
      <div className="zorai-inbox-detail__actions">
        {item.unread ? null : <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => markThreadUnread(item.threadKey)}>Mark unread</button>}
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => archiveThread(item.threadKey, item.at)}>Archive</button>
      </div>
      <ThreadConversation threadKey={item.threadKey} agentName={item.thread.agent_name} title={item.title} refreshKey={item.at} />
    </>
  );
}

function NotificationDetail({ item }: { item: Extract<InboxItem, { kind: "notification" }> }) {
  const markRead = useNotificationStore((state) => state.markRead);
  const markUnread = useNotificationStore((state) => state.markUnread);
  const archiveNotification = useNotificationStore((state) => state.archiveNotification);
  const thread = useAgentStore((state) => (item.threadId ? state.threads.find((entry) => entry.daemonThreadId === item.threadId || entry.id === item.threadId) ?? null : null));
  const notification = item.notification;

  useMarkOnceOnOpen(() => markRead(notification.id));

  return (
    <>
      <div className="zorai-inbox-detail__actions">
        {notification.isRead ? <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => markUnread(notification.id)}>Mark unread</button> : null}
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => archiveNotification(notification.id)}>Archive</button>
      </div>
      {notification.subtitle ? <div className="zorai-inbox-detail__kicker">{notification.subtitle}</div> : null}
      <div className="zorai-inbox-detail__message"><MarkdownContent content={notification.body || "No details."} /></div>
      {canReplyTo(item.threadId) ? (
        <ThreadConversation threadKey={item.threadId} agentName={thread?.agent_name ?? "Agent"} title={thread?.title || item.title} refreshKey={item.at} />
      ) : null}
    </>
  );
}

function UpdatesDetail({ item }: { item: Extract<InboxItem, { kind: "updates" }> }) {
  const markRead = useNotificationStore((state) => state.markRead);
  const markUnread = useNotificationStore((state) => state.markUnread);
  const archiveNotification = useNotificationStore((state) => state.archiveNotification);

  useMarkOnceOnOpen(() => item.notifications.forEach((notification) => markRead(notification.id)));

  return (
    <>
      <div className="zorai-inbox-detail__actions">
        {item.unread ? null : <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => markUnread(item.notifications[0].id)}>Mark unread</button>}
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => item.notifications.forEach((notification) => archiveNotification(notification.id))}>
          Archive all {item.notifications.length}
        </button>
      </div>
      <ol className="zorai-inbox-updates">
        {item.notifications.slice(0, 50).map((notification) => (
          <li key={notification.id}>
            <div className="zorai-inbox-updates__head">
              <strong>{notification.title}</strong>
              <span>{formatWhen(notificationTime(notification))}</span>
            </div>
            {notification.body ? <p>{firstLines(notification.body)}</p> : null}
          </li>
        ))}
      </ol>
      {item.notifications.length > 50 ? <div className="zorai-inbox-detail__note">Showing the latest 50 of {item.notifications.length} updates.</div> : null}
    </>
  );
}

function Composer({ value, onChange, onSubmit, busy, placeholder }: { value: string; onChange: (value: string) => void; onSubmit: () => void; busy: boolean; placeholder: string }) {
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      onSubmit();
    }
  };
  return (
    <div className="zorai-inbox-composer">
      <textarea
        className="zorai-input"
        rows={3}
        value={value}
        disabled={busy}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
      />
      <button type="button" className="zorai-primary-button" disabled={busy || !value.trim()} onClick={onSubmit}>{busy ? "Sending…" : "Send"}</button>
    </div>
  );
}

function detailKicker(item: InboxItem): string {
  switch (item.kind) {
    case "approval": return "Approval needed";
    case "question": return "Question";
    case "goal": return "Goal waiting for approval";
    case "reply": return "Thread reply";
    case "notification": return item.section === "needs" ? "Needs attention" : "Message";
    case "updates": return `${item.notifications.length} update${item.notifications.length === 1 ? "" : "s"}`;
  }
}

function firstLines(text: string): string {
  const lines = text.split("\n").filter((line) => line.trim());
  return lines.slice(0, 4).join("\n");
}
