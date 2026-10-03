declare global {
    type ZoraiOpenAICodexAuthStatus = {
        available: boolean;
        status?: string;
        authMode?: string;
        accountId?: string;
        expiresAt?: number;
        source?: string;
        error?: string;
    };

    type ZoraiOpenAICodexAuthLogin = ZoraiOpenAICodexAuthStatus & {
        authUrl?: string;
    };

    type ZoraiCodingAgentCheck = {
        label: string;
        path: string;
        exists: boolean;
    };

    type ZoraiCodingAgentDiscoveryResult = {
        id: string;
        available: boolean;
        executable: string | null;
        path: string | null;
        version: string | null;
        error?: string | null;
        readiness?: "ready" | "needs-setup" | "missing";
        checks?: ZoraiCodingAgentCheck[];
        runtimeNotes?: string[];
        gatewayLabel?: string | null;
        gatewayReachable?: boolean | null;
    };

    type ZoraiAITrainingCheck = {
        label: string;
        path: string;
        exists: boolean;
        scope: "system" | "workspace";
    };

    type ZoraiAITrainingDiscoveryResult = {
        id: string;
        available: boolean;
        executable: string | null;
        path: string | null;
        version: string | null;
        readiness: "ready" | "needs-setup" | "missing";
        checks: ZoraiAITrainingCheck[];
        runtimeNotes?: string[];
        workspacePath?: string | null;
        error?: string | null;
    };

    type ZoraiSetupDependency = {
        name: string;
        label: string;
        command: string;
        found: boolean;
        path: string | null;
        installHints: string[];
    };

    type ZoraiSetupPrereqReport = {
        profile: "source" | "desktop";
        platform: string;
        required: ZoraiSetupDependency[];
        optional: ZoraiSetupDependency[];
        missingRequired: string[];
        daemonPath: string;
        cliPath: string;
        installRoot: string;
        dataDir: string;
        gettingStartedPath: string;
        whatIsZorai: string;
    };

    type ZoraiInstalledPluginRecord = {
        packageName: string;
        packageVersion: string;
        pluginName: string;
        entryPath: string;
        format: string;
        installedAt: number;
    };

    type ZoraiInstalledPluginLoadResult = {
        packageName: string;
        pluginName: string;
        status: "loaded" | "already-loaded" | "skipped" | "error";
        error?: string;
    };

    type ZoraiGoalRunStatus =
        | "queued"
        | "planning"
        | "running"
        | "awaiting_approval"
        | "awaiting_review"
        | "paused"
        | "completed"
        | "failed"
        | "cancelled";

    type ZoraiGoalRunControlAction = "pause" | "resume" | "cancel" | "accept" | "soft_reject" | "hard_reject";

    type ZoraiTodoItem = {
        id: string;
        content: string;
        status: "pending" | "in_progress" | "completed" | "blocked";
        position: number;
        step_index?: number | null;
        created_at?: number | null;
        updated_at?: number | null;
    };

    type ZoraiWorkContextEntry = {
        path: string;
        previous_path?: string | null;
        kind?: "repo_change" | "artifact" | "generated_skill" | null;
        source: string;
        change_kind?: string | null;
        repo_root?: string | null;
        goal_run_id?: string | null;
        step_index?: number | null;
        session_id?: string | null;
        operation_id?: string | null;
        task_id?: string | null;
        before_hash?: string | null;
        after_hash?: string | null;
        is_text?: boolean;
        updated_at: number;
    };

    type ZoraiThreadWorkContext = {
        thread_id: string;
        entries: ZoraiWorkContextEntry[];
    };

    type ZoraiAgentThread = {
        id: string;
        agent_name?: string | null;
        profile_provider?: string | null;
        profile_model?: string | null;
        profile_reasoning_effort?: string | null;
        profile_context_window_tokens?: number | null;
        title: string;
        created_at?: number | null;
        updated_at?: number | null;
        total_message_count?: number | null;
        loaded_message_start?: number | null;
        loaded_message_end?: number | null;
        active_context_window_start?: number | null;
        active_context_window_end?: number | null;
        active_context_window_tokens?: number | null;
    };

    type ZoraiThreadMessagePinResult = {
        ok: boolean;
        thread_id: string;
        message_id: string;
        error?: string | null;
        current_pinned_chars: number;
        pinned_budget_chars: number;
        candidate_pinned_chars?: number | null;
    };

    type ZoraiGoalRunStep = {
        id: string;
        title: string;
        kind: string;
        status?: string | null;
        success_condition?: string | null;
        session_id?: string | null;
    };

    type ZoraiGoalRun = {
        id: string;
        title: string;
        goal: string;
        client_request_id?: string | null;
        status: ZoraiGoalRunStatus;
        priority?: string | null;
        created_at: number;
        started_at?: number | null;
        completed_at?: number | null;
        thread_id?: string | null;
        current_step_index?: number | null;
        current_step_title?: string | null;
        current_step_kind?: string | null;
        replan_count?: number | null;
        plan_summary?: string | null;
        reflection_summary?: string | null;
        result?: string | null;
        error?: string | null;
        last_error?: string | null;
        failure_cause?: string | null;
        memory_updates?: string[];
        generated_skill_path?: string | null;
        child_task_ids?: string[];
        child_task_count?: number | null;
        approval_count?: number | null;
        duration_ms?: number | null;
        session_id?: string | null;
        awaiting_approval_id?: string | null;
        active_task_id?: string | null;
        total_prompt_tokens?: number | null;
        total_completion_tokens?: number | null;
        estimated_cost_usd?: number | null;
        model_usage?: {
            provider: string;
            model: string;
            request_count: number;
            prompt_tokens: number;
            completion_tokens: number;
            estimated_cost_usd?: number | null;
            duration_ms?: number | null;
        }[];
        launch_assignment_snapshot?: {
            role_id: string;
            enabled: boolean;
            provider: string;
            model: string;
            reasoning_effort?: string | null;
            inherit_from_main: boolean;
        }[];
        runtime_assignment_list?: {
            role_id: string;
            enabled: boolean;
            provider: string;
            model: string;
            reasoning_effort?: string | null;
            inherit_from_main: boolean;
        }[];
        planner_owner_profile?: {
            agent_label: string;
            provider: string;
            model: string;
            reasoning_effort?: string | null;
        } | null;
        current_step_owner_profile?: {
            agent_label: string;
            provider: string;
            model: string;
            reasoning_effort?: string | null;
        } | null;
        steps?: ZoraiGoalRunStep[];
    };

    type ZoraiAgentThreadHandoffPayload = {
        threadId: string;
        action: string;
        targetAgentId?: string | null;
        reason: string;
        summary: string;
        sessionId?: string | null;
    };

    type ZoraiAgentThreadHandoffResult = {
        ok: boolean;
        thread_id: string;
        active_agent_id?: string | null;
        stack_depth?: number | null;
        error?: string | null;
    };

    type ZoraiOperationStatusSnapshot = {
        operation_id: string;
        kind: string;
        dedup?: string | null;
        state: "accepted" | "started" | "completed" | "failed" | string;
        revision: number;
    };

    type ZoraiAgentRunStatus =
        | "queued"
        | "in_progress"
        | "awaiting_approval"
        | "blocked"
        | "failed_analyzing"
        | "budget_exceeded"
        | "completed"
        | "failed"
        | "cancelled";

    type ZoraiAgentRunPriority = "low" | "normal" | "high" | "urgent";

    type ZoraiAgentRun = {
        id: string;
        task_id: string;
        kind: "task" | "subagent";
        classification: "coding" | "research" | "ops" | "browser" | "messaging" | "mixed" | string;
        title: string;
        description: string;
        status: ZoraiAgentRunStatus;
        priority: ZoraiAgentRunPriority;
        progress: number;
        created_at: number;
        started_at?: number | null;
        completed_at?: number | null;
        thread_id?: string | null;
        session_id?: string | null;
        workspace_id?: string | null;
        source: string;
        runtime?: string | null;
        goal_run_id?: string | null;
        goal_run_title?: string | null;
        goal_step_id?: string | null;
        goal_step_title?: string | null;
        parent_run_id?: string | null;
        parent_task_id?: string | null;
        parent_thread_id?: string | null;
        parent_title?: string | null;
        blocked_reason?: string | null;
        error?: string | null;
        result?: string | null;
        last_error?: string | null;
    };

    type ZoraiStatisticsWindow = "today" | "7d" | "30d" | "all";

    type ZoraiAgentStatisticsTotals = {
        input_tokens: number;
        output_tokens: number;
        total_tokens: number;
        cost_usd: number;
        provider_count: number;
        model_count: number;
    };

    type ZoraiProviderStatisticsRow = {
        provider: string;
        input_tokens: number;
        output_tokens: number;
        total_tokens: number;
        cost_usd: number;
    };

    type ZoraiModelStatisticsRow = {
        provider: string;
        model: string;
        input_tokens: number;
        output_tokens: number;
        total_tokens: number;
        cost_usd: number;
    };

    type ZoraiDailyStatisticsRow = {
        day_start: number;
        day_key: string;
        input_tokens: number;
        output_tokens: number;
        total_tokens: number;
        cost_usd: number;
        request_count: number;
    };

    type ZoraiSessionStatisticsRow = {
        thread_id: string;
        title: string;
        updated_at: number;
        provider_models: string[];
        request_count: number;
        input_tokens: number;
        output_tokens: number;
        total_tokens: number;
        cost_usd: number;
    };

    type ZoraiAgentStatisticsSnapshot = {
        window: ZoraiStatisticsWindow;
        generated_at: number;
        has_incomplete_cost_history: boolean;
        totals: ZoraiAgentStatisticsTotals;
        providers: ZoraiProviderStatisticsRow[];
        models: ZoraiModelStatisticsRow[];
        top_models_by_tokens: ZoraiModelStatisticsRow[];
        top_models_by_cost: ZoraiModelStatisticsRow[];
        daily: ZoraiDailyStatisticsRow[];
        sessions: ZoraiSessionStatisticsRow[];
        session_total: number;
        session_limit: number;
        session_offset: number;
    };

    type ZoraiWorkspaceOperator = "user" | "svarog";
    type ZoraiWorkspaceTaskType = "thread" | "goal";
    type ZoraiWorkspaceTaskStatus = "todo" | "in_progress" | "in_review" | "done";
    type ZoraiWorkspacePriority = "low" | "normal" | "high" | "urgent";
    type ZoraiWorkspaceActor = "user" | "svarog" | { agent: string } | { subagent: string } | null;

    type ZoraiWorkspaceSettings = {
        workspace_id: string;
        workspace_root?: string | null;
        operator: ZoraiWorkspaceOperator;
        repo_monitor_enabled?: boolean;
        repo_monitor_include_dirs?: string[];
        repo_monitor_exclude_dirs?: string[];
        created_at: number;
        updated_at: number;
    };

    type ZoraiWorkspaceTaskCreate = {
        workspace_id: string;
        title: string;
        task_type: ZoraiWorkspaceTaskType;
        description: string;
        definition_of_done?: string | null;
        priority?: ZoraiWorkspacePriority | null;
        assignee?: ZoraiWorkspaceActor;
        reviewer?: ZoraiWorkspaceActor;
    };

    type ZoraiWorkspaceTaskUpdate = Partial<{
        title: string;
        description: string;
        definition_of_done: string | null;
        priority: ZoraiWorkspacePriority;
        assignee: ZoraiWorkspaceActor;
        reviewer: ZoraiWorkspaceActor;
    }>;

    type ZoraiWorkspaceTaskMove = {
        task_id: string;
        status: ZoraiWorkspaceTaskStatus;
        sort_order?: number | null;
    };

    type ZoraiWorkspaceFile = {
        path: string;
        content: string;
        hash: string;
        sizeBytes: number;
        modifiedAt: number;
        language: string;
    };

    type ZoraiWorkspaceEntry = {
        name: string;
        path: string;
        isDirectory: boolean;
        isSymbolicLink: boolean;
        sizeBytes: number | null;
        modifiedAt: number | null;
    };

    type ZoraiWorkspaceGitStatus = {
        path: string;
        previousPath: string | null;
        indexStatus: string;
        worktreeStatus: string;
    };

    type ZoraiWorkspaceTest = {
        id: string;
        framework: "rust" | "javascript" | "python" | "go";
        path: string;
        name: string;
        line: number;
        selector: string;
    };

    type ZoraiWorkspaceTestResult = {
        name: string;
        status: "passed" | "failed" | "skipped";
        message: string | null;
        location: { path: string; line: number; column: number } | null;
    };

    type ZoraiWorkspaceTestEvidence = {
        framework: string;
        request: { framework: string; path?: string; selector?: string };
        command: string;
        args: string[];
        status: string;
        exitCode: number | null;
        durationMs: number;
        results: ZoraiWorkspaceTestResult[];
        passed: number;
        failed: number;
        skipped: number;
        output: string;
    };

    type ZoraiWorkspaceTestEvent = {
        runId: string;
        type: "output" | "finished";
        stream?: "stdout" | "stderr";
        text?: string;
        status?: "passed" | "failed" | "cancelled" | "error";
        exitCode?: number | null;
        durationMs?: number;
        output?: string;
        error?: string;
        evidence?: ZoraiWorkspaceTestEvidence;
    };

    type ZoraiLspDiagnostic = {
        message: string;
        severity: number;
        source: string | null;
        code: string | null;
        startLine: number;
        startColumn: number;
        endLine: number;
        endColumn: number;
    };

    type ZoraiLspDiagnosticsPayload = {
        root: string;
        language: string;
        path: string;
        version: number | null;
        diagnostics: ZoraiLspDiagnostic[];
    };

    type ZoraiWorktreeReview = {
        source: { path: string; branch: string | null; head: string; clean: boolean };
        target: { path: string; head: string; clean: boolean };
        mergeBase: string;
        commits: Array<{ hash: string; subject: string }>;
        files: Array<{ status: string; path: string; previousPath: string | null }>;
        canIntegrate: boolean;
    };

    type ZoraiGitWorktree = {
        path: string;
        head: string | null;
        branch: string | null;
        detached: boolean;
        bare: boolean;
        locked: boolean;
        prunable: boolean;
    };

    type ZoraiWorkspaceGitOverview = {
        isRepository: boolean;
        root: string;
        gitRoot: string | null;
        branch: string | null;
        upstream: string | null;
        ahead: number;
        behind: number;
        stagedFiles: number;
        unstagedFiles: number;
    };

    type ZoraiWorkspaceGitHunk = {
        id: string;
        index: number;
        path: string;
        staged: boolean;
        header: string;
        section: string;
        oldStart: number;
        oldLines: number;
        newStart: number;
        newLines: number;
        additions: number;
        deletions: number;
        preview: string;
        patch: string;
    };

    type ZoraiBridge = {
        checkSetupPrereqs?: (profile?: "source" | "desktop") => Promise<ZoraiSetupPrereqReport>;
        discoverCodingAgents?: () => Promise<ZoraiCodingAgentDiscoveryResult[]>;
        discoverAITraining?: (workspacePath?: string | null) => Promise<ZoraiAITrainingDiscoveryResult[]>;
        getDataDir?: () => Promise<string>;
        readJsonFile?: (relativePath: string) => Promise<unknown>;
        writeJsonFile?: (relativePath: string, data: unknown) => Promise<boolean>;
        readTextFile?: (relativePath: string) => Promise<string | null>;
        writeTextFile?: (relativePath: string, content: string) => Promise<boolean>;
        deleteDataPath?: (relativePath: string) => Promise<boolean>;
        listDataDir?: (relativeDir?: string) => Promise<Array<{ name: string; path: string; isDirectory: boolean }>>;
        openDataPath?: (relativePath: string) => Promise<string>;
        revealDataPath?: (relativePath: string) => Promise<boolean>;
        listFsDir?: (targetDir: string) => Promise<Array<{ name: string; path: string; isDirectory: boolean; sizeBytes?: number | null; modifiedAt?: number | null }>>;
        copyFsPath?: (sourcePath: string, destinationPath: string) => Promise<boolean>;
        moveFsPath?: (sourcePath: string, destinationPath: string) => Promise<boolean>;
        deleteFsPath?: (targetPath: string) => Promise<boolean>;
        createFsDirectory?: (targetDirPath: string) => Promise<boolean>;
        openFsPath?: (targetPath: string) => Promise<string>;
        revealFsPath?: (targetPath: string) => Promise<boolean>;
        readFsText?: (targetPath: string) => Promise<string | null>;
        writeFsText?: (targetPath: string, content: string) => Promise<boolean>;
        getFsPathInfo?: (targetPath: string) => Promise<{ path: string; isDirectory: boolean; sizeBytes: number; modifiedAt: number; createdAt: number } | null>;
        gitStatus?: (targetPath: string) => Promise<string>;
        gitDiff?: (targetPath: string, filePath?: string | null) => Promise<string>;
        workspaceOpen?: (rootPath: string) => Promise<{ root: string; name: string; gitRoot: string | null; isGitRepository: boolean }>;
        workspaceSelectFolder?: () => Promise<{ canceled: boolean; root: { root: string; name: string; gitRoot: string | null; isGitRepository: boolean } | null }>;
        workspaceSelectFile?: () => Promise<{ canceled: boolean; path: string | null }>;
        workspaceListDirectory?: (rootPath: string, relativePath?: string, options?: { includeIgnored?: boolean }) => Promise<ZoraiWorkspaceEntry[]>;
        workspaceStatFile?: (rootPath: string, relativePath: string) => Promise<{ path: string; sizeBytes: number; modifiedAt: number }>;
        workspaceReadFile?: (rootPath: string, relativePath: string, options?: { maxBytes?: number }) => Promise<ZoraiWorkspaceFile>;
        workspaceWriteFile?: (rootPath: string, relativePath: string, content: string, expectedHash?: string | null) => Promise<ZoraiWorkspaceFile>;
        workspaceCreateDirectory?: (rootPath: string, relativePath: string) => Promise<{ path: string }>;
        workspaceRenamePath?: (rootPath: string, fromPath: string, toPath: string) => Promise<{ from: string; to: string }>;
        workspaceDeletePath?: (rootPath: string, relativePath: string, options?: { recursive?: boolean }) => Promise<boolean>;
        workspaceGitStatus?: (rootPath: string) => Promise<ZoraiWorkspaceGitStatus[]>;
        workspaceGitOverview?: (rootPath: string) => Promise<ZoraiWorkspaceGitOverview>;
        workspaceGitCommit?: (rootPath: string, message: string) => Promise<{ commit: string; subject: string; overview: ZoraiWorkspaceGitOverview; status: ZoraiWorkspaceGitStatus[] }>;
        workspaceGitHistory?: (rootPath: string, options?: { limit?: number; graph?: boolean }) => Promise<Array<{ hash: string; shortHash: string; author: string; date: string; subject: string; parents?: string[]; refs?: string[] }>>;
        workspaceGitCommitDetail?: (rootPath: string, commitHash: string) => Promise<{ hash: string; author: string; date: string; subject: string; body: string; files: Array<{ status: string; path: string }> }>;
        workspaceGitConflicts?: (rootPath: string) => Promise<Array<{ path: string }>>;
        workspaceGitListWorktrees?: (rootPath: string) => Promise<ZoraiGitWorktree[]>;
        workspaceGitCreateWorktree?: (rootPath: string, options: { name: string; branch: string; baseRef?: string }) => Promise<{ root: string; branch: string; baseRef: string; worktrees: ZoraiGitWorktree[] }>;
        workspaceGitRemoveWorktree?: (rootPath: string, worktreePath: string) => Promise<ZoraiGitWorktree[]>;
        workspaceGitReviewWorktree?: (rootPath: string, worktreePath: string) => Promise<ZoraiWorktreeReview>;
        workspaceGitIntegrateWorktree?: (rootPath: string, worktreePath: string, commitHashes: string[]) => Promise<{ integratedCommits: string[]; overview: ZoraiWorkspaceGitOverview; status: ZoraiWorkspaceGitStatus[]; review: ZoraiWorktreeReview }>;
        workspaceGitStage?: (rootPath: string, relativePath: string) => Promise<ZoraiWorkspaceGitStatus[]>;
        workspaceGitStageMany?: (rootPath: string, relativePaths: string[]) => Promise<ZoraiWorkspaceGitStatus[]>;
        workspaceGitUnstage?: (rootPath: string, relativePath: string) => Promise<ZoraiWorkspaceGitStatus[]>;
        workspaceGitUnstageMany?: (rootPath: string, relativePaths: string[]) => Promise<ZoraiWorkspaceGitStatus[]>;
        workspaceGitBranches?: (rootPath: string) => Promise<Array<{ name: string; isCurrent: boolean }>>;
        workspaceGitCheckout?: (rootPath: string, branch: string) => Promise<{ overview: ZoraiWorkspaceGitOverview; status: ZoraiWorkspaceGitStatus[] }>;
        workspaceGitDiscard?: (rootPath: string, relativePath: string) => Promise<ZoraiWorkspaceGitStatus[]>;
        workspaceGitHunks?: (rootPath: string, relativePath: string, options?: { staged?: boolean }) => Promise<ZoraiWorkspaceGitHunk[]>;
        workspaceGitApplyHunk?: (rootPath: string, relativePath: string, hunkId: string, action: "stage" | "unstage" | "discard") => Promise<{ status: ZoraiWorkspaceGitStatus[]; hunks: ZoraiWorkspaceGitHunk[] }>;
        workspaceSearch?: (rootPath: string, query: string, options?: { caseSensitive?: boolean; maxResults?: number; maxFiles?: number }) => Promise<Array<{ path: string; line: number; column: number; preview: string }>>;
        workspaceGitDiff?: (rootPath: string, relativePath?: string | null, options?: { staged?: boolean; againstHead?: boolean; includeUntracked?: boolean }) => Promise<string>;
        workspaceGitShow?: (rootPath: string, relativePath: string, revision?: string) => Promise<string>;
        workspaceWatchStart?: (rootPath: string, options?: { debounceMs?: number; maxDirectories?: number }) => Promise<{ subscriptionId: string; root: string; watchedDirectoryCount: number }>;
        workspaceWatchStop?: (subscriptionId: string) => Promise<boolean>;
        onWorkspaceFilesChanged?: (cb: (batch: { subscriptionId: string; root: string; changes: Array<{ path: string; eventType: "change" | "rename"; observedAt: number }>; emittedAt: number }) => void) => (() => void) | void;
        workspaceLspStatus?: (rootPath: string, language: string) => Promise<{ root: string; language: string; configured: boolean; available: boolean; command: string | null; server: string | null }>;
        workspaceLspOpen?: (rootPath: string, relativePath: string, language: string, content: string, version: number) => Promise<{ available: boolean; reason?: string; root?: string; server?: string; command?: string; document?: { path: string; version: number } }>;
        workspaceLspChange?: (rootPath: string, relativePath: string, language: string, content: string, version: number) => Promise<unknown>;
        workspaceLspRequest?: (rootPath: string, relativePath: string, language: string, method: "hover" | "definition" | "references" | "completion", position: { line: number; character: number }) => Promise<{ available: boolean; reason?: string; result?: unknown }>;
        workspaceLspClose?: (rootPath: string, relativePath: string, language: string) => Promise<boolean>;
        workspaceLspUnsubscribe?: (rootPath: string, language: string) => Promise<boolean>;
        onWorkspaceLspDiagnostics?: (cb: (payload: ZoraiLspDiagnosticsPayload) => void) => (() => void) | void;
        workspaceTestsDiscover?: (rootPath: string, options?: { maxTests?: number }) => Promise<{ root: string; frameworks: Array<{ id: string; label: string; command: string }>; tests: ZoraiWorkspaceTest[]; truncated: boolean }>;
        workspaceTestsRun?: (rootPath: string, request: { framework: string; path?: string; selector?: string }) => Promise<{ runId: string; command: string; args: string[]; startedAt: number }>;
        workspaceTestsCancel?: (runId: string) => Promise<boolean>;
        onWorkspaceTestEvent?: (cb: (payload: ZoraiWorkspaceTestEvent) => void) => (() => void) | void;
        dbAppendCommandLog?: (entry: unknown) => Promise<boolean>;
        dbCompleteCommandLog?: (id: string, exitCode?: number | null, durationMs?: number | null) => Promise<boolean>;
        dbQueryCommandLog?: (opts?: { workspaceId?: string | null; paneId?: string | null; limit?: number | null }) => Promise<unknown[]>;
        dbClearCommandLog?: () => Promise<boolean>;
        dbCreateThread?: (thread: unknown) => Promise<boolean>;
        dbDeleteThread?: (id: string) => Promise<boolean>;
        dbListThreads?: () => Promise<unknown[]>;
        dbGetThread?: (id: string) => Promise<{ thread: unknown; messages: unknown[] }>;
        dbAddMessage?: (message: unknown) => Promise<boolean>;
        dbExportThread?: (threadId: string, messageId: string) => Promise<{ ok: boolean; file_path?: string | null; error?: string }>;
        dbForkThread?: (threadId: string, messageId: string) => Promise<{ ok: boolean; thread_id?: string | null; title?: string | null; error?: string }>;
        dbDeleteMessage?: (threadId: string, messageId: string) => Promise<boolean>;
        dbListMessages?: (threadId: string, limit?: number | null) => Promise<unknown[]>;
        agentAddTask?: (payload: { title: string; description: string; priority?: string; command?: string | null; sessionId?: string | null; scheduledAt?: number | null; dependencies?: string[] }) => Promise<unknown>;
        agentListRuns?: (parentThreadId?: string | null) => Promise<ZoraiAgentRun[] | unknown>;
        agentGetRun?: (runId: string) => Promise<ZoraiAgentRun | null | unknown>;
        agentListTodos?: () => Promise<Record<string, ZoraiTodoItem[]> | unknown>;
        agentGetTodos?: (threadId: string) => Promise<{ thread_id: string; items: ZoraiTodoItem[] } | ZoraiTodoItem[] | unknown>;
        agentGetWorkContext?: (threadId: string) => Promise<{ thread_id: string; context: ZoraiThreadWorkContext } | ZoraiThreadWorkContext | null | unknown>;
        agentGetFileOperationSnapshot?: (operationId: string) => Promise<{ operation_id: string; status: { operation_id: string; available: boolean; revertible: boolean; stale_paths: string[]; retained_bytes: number; entries: Array<{ path: string; before_hash: string | null; after_hash: string | null; before_existed: boolean; snapshot_file: string | null }>; reason: string | null } }>;
        agentRevertFileOperation?: (operationId: string) => Promise<{ operation_id?: string; result?: { operation_id: string; reverted_paths: string[] }; ok?: boolean; error?: string }>;
        agentGetThreadWorkspaceContext?: (threadId: string) => Promise<{ thread_id: string; context: unknown; updated: boolean }>;
        agentSetThreadWorkspaceContext?: (threadId: string, context: unknown) => Promise<{ thread_id: string; context: unknown; updated: boolean; error?: string }>;
        agentSpawnSubagent?: (threadId: string, request: { title: string; description: string; cwd?: string | null; session?: string | null; priority?: string; provider?: string; model?: string; reasoning_effort?: string; runtime?: string; max_depth?: number; budget?: { max_tokens?: number; max_wall_time_secs?: number } }) => Promise<{ ok: boolean; content?: string; error?: string }>;
        agentGetGitDiff?: (repoPath: string, filePath?: string | null) => Promise<{ repo_path: string; file_path?: string | null; diff: string } | string | unknown>;
        agentGetFilePreview?: (path: string, maxBytes?: number | null) => Promise<{ path: string; content: string; truncated: boolean; is_text: boolean } | null | unknown>;
        agentStartGoalRun?: (payload: { goal: string; title?: string | null; sessionId?: string | null; priority?: string | null; threadId?: string | null; targetAgentId?: string | null; clientRequestId?: string | null; requiresApproval?: boolean; launchAssignments?: Array<{ role_id: string; enabled: boolean; provider: string; model: string; reasoning_effort?: string | null; inherit_from_main: boolean }> }) => Promise<ZoraiGoalRun | unknown>;
        agentListGoalRuns?: () => Promise<ZoraiGoalRun[] | unknown>;
        agentGetGoalRun?: (goalRunId: string) => Promise<ZoraiGoalRun | unknown>;
        agentControlGoalRun?: (goalRunId: string, action: ZoraiGoalRunControlAction, stepIndex?: number | null, payloadJson?: string | null) => Promise<boolean | { ok?: boolean; success?: boolean } | unknown>;
        agentListWorkspaceSettings?: () => Promise<ZoraiWorkspaceSettings[] | unknown>;
        agentGetWorkspaceSettings?: (workspaceId: string) => Promise<ZoraiWorkspaceSettings | unknown>;
        agentSetWorkspaceOperator?: (workspaceId: string, operator: ZoraiWorkspaceOperator) => Promise<ZoraiWorkspaceSettings | unknown>;
        agentSetWorkspaceRepoMonitor?: (workspaceId: string, payload: { repo_monitor_enabled: boolean; repo_monitor_include_dirs: string[]; repo_monitor_exclude_dirs: string[] }) => Promise<ZoraiWorkspaceSettings | unknown>;
        agentListWorkspaceTasks?: (workspaceId: string, includeDeleted?: boolean) => Promise<unknown[] | unknown>;
        agentCreateWorkspaceTask?: (request: ZoraiWorkspaceTaskCreate) => Promise<unknown>;
        agentUpdateWorkspaceTask?: (taskId: string, update: ZoraiWorkspaceTaskUpdate) => Promise<unknown>;
        agentMoveWorkspaceTask?: (request: ZoraiWorkspaceTaskMove) => Promise<unknown>;
        agentRunWorkspaceTask?: (taskId: string) => Promise<unknown>;
        agentPauseWorkspaceTask?: (taskId: string) => Promise<unknown>;
        agentStopWorkspaceTask?: (taskId: string) => Promise<unknown>;
        agentDeleteWorkspaceTask?: (taskId: string) => Promise<unknown>;
        agentListWorkspaceNotices?: (workspaceId: string, taskId?: string | null) => Promise<unknown[] | unknown>;
        agentExplainAction?: (actionId: string, stepIndex?: number | null) => Promise<unknown>;
        agentStartDivergentSession?: (payload: {
            problemStatement: string;
            threadId: string;
            goalRunId?: string | null;
            customFramingsJson?: string | null;
        }) => Promise<unknown>;
        agentGetDivergentSession?: (sessionId: string) => Promise<unknown>;
        dbUpsertTranscriptIndex?: (entry: unknown) => Promise<boolean>;
        dbListTranscriptIndex?: (workspaceId?: string | null) => Promise<unknown[]>;
        dbUpsertSnapshotIndex?: (entry: unknown) => Promise<boolean>;
        dbListSnapshotIndex?: (workspaceId?: string | null) => Promise<unknown[]>;
        dbUpsertAgentEvent?: (eventRow: unknown) => Promise<boolean>;
        dbListAgentEvents?: (opts?: { category?: string | null; paneId?: string | null; limit?: number | null }) => Promise<unknown[]>;
        dbListDatabaseTables?: () => Promise<unknown[]>;
        dbQueryDatabaseRows?: (opts: { tableName: string; offset?: number; limit?: number; sortColumn?: string | null; sortDirection?: "asc" | "desc" | null }) => Promise<unknown>;
        dbUpdateDatabaseRows?: (tableName: string, updates: unknown[]) => Promise<{ updatedRows?: number; error?: string } | unknown>;
        dbExecuteDatabaseSql?: (sql: string) => Promise<unknown>;
        dbGetBackend?: () => Promise<{ backend?: string | null; syncUrl?: string | null; hasToken?: boolean; seededAt?: number | null; error?: string }>;
        dbSyncNow?: () => Promise<{ ok?: boolean; message?: string }>;
        dbQueueSemanticBackfill?: (limit?: number | null) => Promise<unknown>;
        dbGetSemanticIndexStatus?: (opts: { embeddingModel: string; dimensions: number }) => Promise<unknown>;
        startTerminalSession?: (options: {
            paneId: string;
            sessionId?: string | null;
            shell?: string | null;
            cwd?: string | null;
            workspaceId?: string | null;
            cols?: number;
            rows?: number;
            sourcePaneId?: string | null;
        }) => Promise<{ sessionId?: string | null; initialOutput?: string[]; state?: string }>;
        onTerminalEvent?: (cb: (event: any) => void) => (() => void) | void;
        sendTerminalInput?: (paneId: string | null, data: string) => Promise<boolean>;
        cloneTerminalSession?: (payload: {
            sourcePaneId?: string;
            sourceSessionId?: string | null;
            workspaceId?: string | null;
            cwd?: string | null;
            cols?: number;
            rows?: number;
        }) => Promise<{ sessionId: string; activeCommand?: string }>;
        stopTerminalSession?: (paneId: string, killSession?: boolean) => Promise<boolean>;
        listBusyTerminalPanes?: () => Promise<string[]>;
        executeManagedCommand?: (paneId: string | null, payload: unknown) => Promise<boolean | { output?: string }>;
        agentSendMessage?: (threadId: string | null, content: string, sessionId?: string | null, contextMessages?: unknown[], contentBlocksJson?: string | null, targetAgentId?: string | null) => Promise<{ ok?: boolean; error?: string } | unknown>;
        agentEnqueuePrompt?: (payload: {
            threadId: string;
            content: string;
            contentBlocksJson?: string | null;
            promptId?: string | null;
        }) => Promise<{ thread_id?: string | null; prompts?: unknown[]; error?: string } | unknown>;
        agentListPromptQueue?: (threadId?: string | null) => Promise<{ thread_id?: string | null; prompts?: unknown[]; error?: string } | unknown>;
        agentUpdateQueuedPrompt?: (payload: {
            threadId: string;
            promptId: string;
            content: string;
            contentBlocksJson?: string | null;
        }) => Promise<{ thread_id?: string | null; prompts?: unknown[]; error?: string } | unknown>;
        agentCancelQueuedPrompt?: (payload: {
            threadId: string;
            promptId: string;
        }) => Promise<{ thread_id?: string | null; prompts?: unknown[]; error?: string } | unknown>;
        agentSendQueuedPromptNow?: (payload: {
            threadId: string;
            promptId: string;
        }) => Promise<{ thread_id?: string | null; prompts?: unknown[]; error?: string } | unknown>;
        agentInternalDelegate?: (threadId: string | null, targetAgentId: string, content: string, sessionId?: string | null) => Promise<{ ok?: boolean; error?: string } | unknown>;
        agentThreadParticipantCommand?: (payload: {
            threadId: string;
            targetAgentId: string;
            action: string;
            instruction?: string | null;
            sessionId?: string | null;
        }) => Promise<{ ok?: boolean; error?: string } | unknown>;
        agentSendParticipantSuggestion?: (payload: {
            threadId: string;
            suggestionId: string;
            sessionId?: string | null;
            forceSend?: boolean;
        }) => Promise<{ ok?: boolean; error?: string } | unknown>;
        agentHandoffThread?: (payload: ZoraiAgentThreadHandoffPayload) => Promise<ZoraiAgentThreadHandoffResult | { ok?: boolean; thread_id?: string | null; error?: string } | unknown>;
        agentGetOperationStatus?: (id: string) => Promise<ZoraiOperationStatusSnapshot | { ok?: boolean; operation_id?: string; error?: string } | unknown>;
        agentCancelOperation?: (id: string) => Promise<{ ok?: boolean; error?: string } | unknown>;
        agentDismissParticipantSuggestion?: (payload: {
            threadId: string;
            suggestionId: string;
            sessionId?: string | null;
        }) => Promise<{ ok?: boolean; error?: string } | unknown>;
        agentListThreads?: (options?: {
            agentFilter?: string | null;
            includeInternal?: boolean;
        }) => Promise<unknown[]>;
        agentGetThread?: (
            threadId: string,
            options?: {
                messageLimit?: number | null;
                messageOffset?: number | null;
            },
        ) => Promise<unknown | null>;
        agentPinThreadMessageForCompaction?: (threadId: string, messageId: string) => Promise<ZoraiThreadMessagePinResult | unknown>;
        agentUnpinThreadMessageForCompaction?: (threadId: string, messageId: string) => Promise<ZoraiThreadMessagePinResult | unknown>;
        agentMessageFeedback?: (threadId: string, messageId: string, reaction: "up" | "down" | null, absoluteMessageIndex?: number) => Promise<unknown>;
        agentGetThreadExecutionProfile?: (threadId: string) => Promise<{ thread_id: string; profile: unknown; error?: string } | unknown>;
        agentSetThreadExecutionProfile?: (threadId: string, profile: unknown) => Promise<{ thread_id: string; profile: unknown; error?: string } | unknown>;
        agentForceCompact?: (threadId: string) => Promise<{ ok: boolean; error?: string } | unknown>;
        openAICodexAuthStatus?: (options?: { refresh?: boolean }) => Promise<ZoraiOpenAICodexAuthStatus>;
        openAICodexAuthLogin?: () => Promise<ZoraiOpenAICodexAuthLogin>;
        openAICodexAuthLogout?: () => Promise<{ ok: boolean }>;
        agentFetchModels?: (providerId: string, base_url: string, api_key: string, output_modalities?: string) => Promise<{ models?: Array<{ id: string; name?: string; context_window?: number; pricing?: { prompt?: string; completion?: string; image?: string; request?: string; web_search?: string; internal_reasoning?: string; input_cache_read?: string; input_cache_write?: string; audio?: string }; metadata?: Record<string, unknown> }>; error?: string } | unknown>;
        agentGetProviderAuthStates?: () => Promise<unknown[]>;
        agentGetProviderCatalog?: () => Promise<unknown>;
        agentLoginProvider?: (providerId: string, api_key: string, base_url?: string) => Promise<unknown[] | { error?: string }>;
        agentLogoutProvider?: (providerId: string) => Promise<unknown[] | { error?: string }>;
        agentValidateProvider?: (providerId: string, base_url: string, api_key: string, auth_source: string) => Promise<{ valid: boolean; error?: string; models?: unknown[] }>;
        agentGetConfig?: () => Promise<unknown>;
        agentGetMlflowTracingStatus?: () => Promise<unknown>;
        agentTestMlflowTracingConnection?: () => Promise<unknown>;
        agentSendMlflowTracingTestTrace?: () => Promise<unknown>;
        agentListMlflowTracingHeaders?: () => Promise<{ names?: string[] } | unknown>;
        agentSetMlflowTracingHeader?: (name: string, value: string) => Promise<{ names?: string[] } | unknown>;
        agentDeleteMlflowTracingHeader?: (name: string) => Promise<{ names?: string[] } | unknown>;
        agentExternalRuntimeMigrationStatus?: () => Promise<unknown>;
        agentExternalRuntimeMigrationPreview?: (runtime: "hermes" | "openclaw", configPath?: string | null) => Promise<unknown>;
        agentExternalRuntimeMigrationApply?: (runtime: "hermes" | "openclaw", configPath?: string | null, conflictPolicy?: string) => Promise<unknown>;
        agentExternalRuntimeMigrationReport?: (runtime?: "hermes" | "openclaw" | null, limit?: number | null) => Promise<unknown>;
        agentExternalRuntimeMigrationShadowRun?: (runtime: "hermes" | "openclaw") => Promise<unknown>;
        agentGetStatus?: () => Promise<{
            tier: string;
            feature_flags: unknown;
            activity: string;
            active_thread_id: string | null;
            active_goal_run_id: string | null;
            active_goal_run_title: string | null;
            provider_health: Record<string, { can_execute: boolean; trip_count: number }>;
            gateway_statuses: Record<string, { status: string; consecutive_failures: number }>;
            recent_actions: Array<{ id: number; timestamp: number; action_type: string; summary: string }>;
            diagnostics?: {
                operator_profile_sync_state?: string;
                operator_profile_sync_dirty?: boolean;
                operator_profile_scheduler_fallback?: boolean;
            };
        } | null>;
        agentGetStatistics?: (window?: ZoraiStatisticsWindow, sessionLimit?: number, sessionOffset?: number, sessionsOnly?: boolean) => Promise<ZoraiAgentStatisticsSnapshot | null | unknown>;
        agentInspectPrompt?: (agentId?: string | null) => Promise<{
            agent_id: string;
            agent_name: string;
            provider_id: string;
            model: string;
            sections: Array<{ id: string; title: string; content: string }>;
            final_prompt: string;
        } | null>;
        agentQueryAudits?: (actionTypes?: string[] | null, since?: number | null, limit?: number | null) => Promise<unknown>;
        agentGetProvenanceReport?: (limit?: number | null) => Promise<unknown>;
        agentGetMemoryProvenanceReport?: (target?: string | null, limit?: number | null) => Promise<unknown>;
        agentConfirmMemoryProvenanceEntry?: (entryId: string) => Promise<unknown>;
        agentRetractMemoryProvenanceEntry?: (entryId: string) => Promise<unknown>;
        agentGetCollaborationSessions?: (parentTaskId?: string | null) => Promise<unknown>;
        agentVoteOnCollaborationDisagreement?: (parentTaskId: string, disagreementId: string, taskId: string, position: string, confidence?: number | null) => Promise<unknown>;
        agentListGeneratedTools?: () => Promise<unknown>;
        agentRunGeneratedTool?: (toolName: string, argsJson: string) => Promise<unknown>;
        agentSpeechToText?: (base64Audio: string, mimeType?: string | null, options?: Record<string, unknown> | null) => Promise<unknown>;
        agentTextToSpeech?: (text: string, voice?: string | null, options?: Record<string, unknown> | null) => Promise<unknown>;
        agentGenerateImage?: (prompt: string, options?: Record<string, unknown> | null) => Promise<unknown>;
        agentActivateGeneratedTool?: (toolName: string) => Promise<unknown>;
        agentPromoteGeneratedTool?: (toolName: string) => Promise<unknown>;
        agentRetireGeneratedTool?: (toolName: string) => Promise<unknown>;
        agentGetOperatorModel?: () => Promise<unknown>;
        agentResetOperatorModel?: () => Promise<{ ok?: boolean; error?: string } | unknown>;
        agentSetConfigItem?: (keyPath: string, value: unknown) => Promise<unknown>;
        agentSetProviderModel?: (providerId: string, model: string) => Promise<{ ok?: boolean; error?: string }>;
        agentSetTargetAgentProviderModel?: (targetAgentId: string, providerId: string, model: string) => Promise<{ ok?: boolean; error?: string }>;
        agentSetTargetAgentReasoningEffort?: (targetAgentId: string, reasoningEffort: string) => Promise<{ ok?: boolean; error?: string }>;
        agentSetTargetAgentContextWindow?: (targetAgentId: string, contextWindowTokens: number) => Promise<{ ok?: boolean; error?: string }>;
        agentSetTierOverride?: (tier: string | null) => Promise<unknown>;
        agentSetSubAgent?: (subAgentJson: string) => Promise<{ ok?: boolean; error?: string }>;
        agentRemoveSubAgent?: (subAgentId: string) => Promise<{ ok?: boolean }>;
        agentListSubAgents?: () => Promise<unknown[]>;
        agentGetConciergeConfig?: () => Promise<unknown>;
        agentSetConciergeConfig?: (config: unknown) => Promise<unknown>;
        agentDismissConciergeWelcome?: () => Promise<{ ok?: boolean }>;
        agentRequestConciergeWelcome?: () => Promise<{ ok?: boolean }>;
        listInstalledPlugins?: () => Promise<ZoraiInstalledPluginRecord[]>;
        loadInstalledPlugins?: () => Promise<ZoraiInstalledPluginLoadResult[]>;
        pluginDaemonList?: () => Promise<{ plugins: Array<{ name: string; version: string; description?: string; author?: string; enabled: boolean; install_source: string; has_api: boolean; has_auth: boolean; has_commands: boolean; has_skills: boolean; endpoint_count: number; settings_count: number; installed_at: string; updated_at: string; auth_status?: string }> }>;
        pluginDaemonGet?: (name: string) => Promise<{ plugin: { name: string; version: string; description?: string; author?: string; enabled: boolean; install_source: string; has_api: boolean; has_auth: boolean; has_commands: boolean; has_skills: boolean; endpoint_count: number; settings_count: number; installed_at: string; updated_at: string; auth_status?: string } | null; settings_schema: string | null }>;
        pluginOAuthStart?: (name: string) => Promise<{ name: string; url: string }>;
        onPluginOAuthComplete?: (callback: (data: { name: string; success: boolean; error?: string }) => void) => (() => void) | void;
        pluginDaemonEnable?: (name: string) => Promise<{ ok?: boolean; error?: string }>;
        pluginDaemonDisable?: (name: string) => Promise<{ ok?: boolean; error?: string }>;
        pluginDaemonInstall?: (dirName: string, installSource: string) => Promise<{ ok?: boolean; message?: string; error?: string }>;
        pluginDaemonUninstall?: (name: string) => Promise<{ ok?: boolean; message?: string; error?: string }>;
        pluginGetSettings?: (name: string) => Promise<{ plugin_name: string; settings: Array<{ key: string; value: string; is_secret: boolean }> }>;
        pluginUpdateSettings?: (pluginName: string, key: string, value: string, isSecret: boolean) => Promise<{ ok?: boolean; error?: string }>;
        pluginTestConnection?: (name: string) => Promise<{ plugin_name: string; success: boolean; message: string }>;
        readClipboardText?: () => Promise<string | null>;
        writeClipboardText?: (text: string) => Promise<void>;
        agentResolveTaskApproval?: (approvalId: string, decision: string) => Promise<unknown>;
        agentListTasks?: () => Promise<unknown[]>;
        sendDiscordMessage?: (payload: { token: string; channelId?: string; userId?: string; message: string }) => Promise<{ ok?: boolean; error?: string; channelId?: string; userId?: string; destination?: string }>;
        whatsappSend?: (jid: string, message: string) => Promise<{ ok?: boolean; error?: string }>;
        whatsappConnect?: () => Promise<{ ok?: boolean; error?: string }>;
        whatsappDisconnect?: () => Promise<{ ok?: boolean; error?: string }>;
        whatsappStatus?: () => Promise<{ status?: string; phone?: string; phoneNumber?: string; lastError?: string | null }>;
        onWhatsAppQR?: (cb: (dataUrl: string | null) => void) => (() => void) | void;
        onWhatsAppConnected?: (cb: (info: { phone: string }) => void) => (() => void) | void;
        onWhatsAppDisconnected?: (cb: (info?: { reason?: string | null } | null) => void) => (() => void) | void;
        onWhatsAppError?: (cb: (message: string) => void) => (() => void) | void;
        saveVisionScreenshot?: (payload: { dataUrl: string }) => Promise<{ ok?: boolean; error?: string; path?: string; expiresAt?: number }>;
        getSystemMonitorSnapshot?: (opts?: { processLimit?: number }) => Promise<any>;
        getSystemFonts?: () => Promise<string[]>;
        getAvailableShells?: () => Promise<Array<{ name: string; path: string; args?: string }>>;
        checkDaemon?: () => Promise<boolean>;
        checkLspHealth?: () => Promise<unknown>;
        checkMcpHealth?: (servers: unknown) => Promise<unknown>;
        onAppCommand?: (cb: (command: string) => void) => (() => void) | void;
        verifyIntegrity?: () => Promise<unknown>;
        resolveManagedApproval?: (paneId: string, approvalId: string, decision: string) => Promise<unknown>;
        onAgentEvent?: (cb: (event: any) => void) => (() => void) | void;
        agentStopStream?: (threadId: string) => Promise<unknown>;
        agentRetryStreamNow?: (threadId: string) => Promise<unknown>;
        agentCancelTask?: (taskId: string) => Promise<unknown>;
        agentHeartbeatGetItems?: () => Promise<unknown[]>;
        setWindowOpacity?: (opacity: number) => Promise<void>;
        windowMinimize?: () => void;
        windowMaximize?: () => void;
        windowClose?: () => void;
        windowIsMaximized?: () => Promise<boolean>;
        onWindowState?: (cb: (state: any) => void) => (() => void) | void;
        getPlatform?: () => Promise<string>;
        resizeTerminalSession?: (paneId: string | null, cols: number, rows: number) => Promise<boolean>;
        listSnapshots?: (paneId: string, workspaceId?: string | null) => Promise<unknown[]>;
        restoreSnapshot?: (paneId: string, snapshotId: string) => Promise<unknown>;
        searchManagedHistory?: (paneId: string, query: string, limit?: number) => Promise<unknown>;
        findManagedSymbol?: (paneId: string, workspaceRoot: string, symbol: string, limit?: number) => Promise<unknown>;
        generateManagedSkill?: (paneId: string, query?: string | null, title?: string | null) => Promise<unknown>;
        onWhatsAppMessage?: (cb: (event: any) => void) => (() => void) | void;
        agentStartOperatorProfileSession?: (kind: string) => Promise<{ session_id: string; kind: string } | { error?: string }>;
        agentNextOperatorProfileQuestion?: (sessionId: string) => Promise<{ session_id: string; question_id: string; field_key: string; prompt: string; input_kind: string; optional: boolean } | { error?: string }>;
        agentSubmitOperatorProfileAnswer?: (sessionId: string, questionId: string, answerJson: string) => Promise<{ session_id: string; answered: number; remaining: number; completion_ratio: number } | { error?: string }>;
        agentSkipOperatorProfileQuestion?: (sessionId: string, questionId: string, reason?: string | null) => Promise<{ session_id: string; answered: number; remaining: number; completion_ratio: number } | { error?: string }>;
        agentDeferOperatorProfileQuestion?: (sessionId: string, questionId: string, deferUntilUnixMs?: number | null) => Promise<{ session_id: string; answered: number; remaining: number; completion_ratio: number } | { error?: string }>;
        agentGetOperatorProfileSummary?: () => Promise<unknown>;
        agentSetOperatorProfileConsent?: (consentKey: string, granted: boolean) => Promise<{ ok?: boolean; error?: string }>;
        agentAnswerQuestion?: (questionId: string, answer: string) => Promise<{ ok?: boolean; error?: string }>;
    };

    interface Window {
        zorai?: ZoraiBridge;
    }
}

export { };
