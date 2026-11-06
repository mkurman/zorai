const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const preloadPath = path.join(__dirname, "preload.cjs");
const runtimePath = path.join(__dirname, "agent-query-runtime.cjs");
const handlerPath = path.join(__dirname, "main", "agent-ipc-handlers.cjs");

const preloadSrc = fs.readFileSync(preloadPath, "utf8");
const runtime = require(runtimePath);
const { registerAgentIpcHandlers } = require(handlerPath);

function createHandlerHarness() {
  const handlers = new Map();
  const queries = [];
  const persistedAudio = [];
  const ipcMain = {
    handle(name, handler) {
      handlers.set(name, handler);
    },
  };
  const sendAgentQuery = async (...args) => {
    queries.push(args);
    if (args[1] === "speech-to-text-result") {
      return { text: "transcribed hello" };
    }
    if (args[1] === "text-to-speech-result") {
      return { path: "/tmp/speech.mp3", mime_type: "audio/mpeg" };
    }
    if ([].concat(args[1]).includes("image-generation-result")) {
      return {
        ok: true,
        path: "/tmp/generated-image.png",
        mime_type: "image/png",
        thread_id: "thread-image",
      };
    }
    if ([].concat(args[1]).includes("music-generation-result")) {
      return {
        ok: true,
        path: "/tmp/music-1.mp3",
        mime_type: "audio/mpeg",
        thread_id: "thread-music",
      };
    }
    if ([].concat(args[1]).includes("video-generation-result")) {
      return {
        ok: true,
        path: "/tmp/video-1.mp4",
        mime_type: "video/mp4",
        thread_id: "thread-video",
      };
    }
    return { ok: true };
  };

  registerAgentIpcHandlers(
    ipcMain,
    { sendAgentCommand: () => {}, sendAgentQuery },
    {
      logToFile: () => {},
      openAICodexAuthHandlers: {
        status: async () => ({ available: false }),
        login: async () => ({ available: false }),
        logout: async () => ({ ok: true }),
      },
      saveTempAudioCapture: (payload) => {
        persistedAudio.push(payload);
        return { ok: true, path: "/tmp/capture.webm", mimeType: payload.mimeType || "audio/webm" };
      },
    },
  );

  return { handlers, queries, persistedAudio };
}

test("preload exposes speech bridge methods", () => {
  assert.match(
    preloadSrc,
    /agentSpeechToText:\s*\(base64Audio, mimeType, options\)\s*=>\s*ipcRenderer\.invoke\('agent-speech-to-text', base64Audio, mimeType, options\)/,
  );
  assert.match(
    preloadSrc,
    /agentTextToSpeech:\s*\(text, voice, options\)\s*=>\s*ipcRenderer\.invoke\('agent-text-to-speech', text, voice, options\)/,
  );
  assert.match(
    preloadSrc,
    /agentGenerateImage:\s*\(prompt, options\)\s*=>\s*ipcRenderer\.invoke\('agent-generate-image', prompt, options\)/,
  );
});

test("preload exposes the video generation bridge method", () => {
  assert.match(
    preloadSrc,
    /agentGenerateVideo:\s*\(prompt, options\)\s*=>\s*ipcRenderer\.invoke\('agent-generate-video', prompt, options\)/,
  );
});

test("runtime allowlists the video generation result so the pending query can resolve", () => {
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("video-generation-result"));
});

test("agent-generate-video waits long enough for multi-minute generations and returns a loadable file_url", async () => {
  const { handlers, queries } = createHandlerHarness();

  const result = await handlers.get("agent-generate-video")(null, "drone shot over fjord", {
    thread_id: "thread-video",
    provider: "minimax",
    model: "MiniMax-Hailuo-2.3",
  });

  assert.deepEqual(result, {
    ok: true,
    path: "/tmp/video-1.mp4",
    mime_type: "video/mp4",
    thread_id: "thread-video",
    file_url: "file:///tmp/video-1.mp4",
  });
  assert.equal(queries.length, 1);
  const [command, responseType, timeoutMs] = queries[0];
  assert.deepEqual(command, {
    type: "generate-video",
    args_json: JSON.stringify({
      thread_id: "thread-video",
      provider: "minimax",
      model: "MiniMax-Hailuo-2.3",
      prompt: "drone shot over fjord",
    }),
  });
  assert.deepEqual(
    responseType,
    ["video-generation-result", "error"],
    "a daemon AgentError must reject the query instead of leaving the GUI waiting for the timeout",
  );
  assert.ok(timeoutMs >= 660000, `daemon waits up to 600s, client timeout ${timeoutMs}ms must exceed it`);
});

test("agent-generate-music waits for long songs, rejects on daemon errors and returns a playable file_url", async () => {
  const { handlers, queries } = createHandlerHarness();

  const result = await handlers.get("agent-generate-music")(null, "lofi hip hop", {
    thread_id: "thread-music",
    instrumental: true,
  });

  assert.deepEqual(result, {
    ok: true,
    path: "/tmp/music-1.mp3",
    mime_type: "audio/mpeg",
    thread_id: "thread-music",
    file_url: "file:///tmp/music-1.mp3",
  });
  const [command, responseType, timeoutMs] = queries[0];
  assert.deepEqual(command, {
    type: "generate-music",
    args_json: JSON.stringify({ thread_id: "thread-music", instrumental: true, prompt: "lofi hip hop" }),
  });
  assert.deepEqual(responseType, ["music-generation-result", "error"]);
  assert.ok(timeoutMs >= 660000, `daemon waits up to 600s, client timeout ${timeoutMs}ms must exceed it`);
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("music-generation-result"));
  assert.match(
    preloadSrc,
    /agentGenerateMusic:\s*\(prompt, options\)\s*=>\s*ipcRenderer\.invoke\('agent-generate-music', prompt, options\)/,
  );
});

test("runtime allowlists speech query response types", () => {
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("speech-to-text-result"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("text-to-speech-result"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("image-generation-result"));
});

test("agent IPC handlers route speech APIs through the daemon bridge", async () => {
  const { handlers, queries, persistedAudio } = createHandlerHarness();

  assert.ok(handlers.has("agent-speech-to-text"));
  assert.ok(handlers.has("agent-text-to-speech"));
  assert.ok(handlers.has("agent-generate-image"));

  const sttResult = await handlers.get("agent-speech-to-text")(null, "Zm9v", "audio/webm", {
    provider: "openai",
    model: "whisper-1",
    language: "en",
  });
  const ttsResult = await handlers.get("agent-text-to-speech")(null, "Hello", "alloy", {
    provider: "openai",
    model: "gpt-4o-mini-tts",
    response_format: "mp3",
  });
  const imageResult = await handlers.get("agent-generate-image")(null, "retro robot portrait", {
    thread_id: "thread-image",
    provider: "openai",
    model: "gpt-image-1",
  });

  assert.deepEqual(sttResult, { text: "transcribed hello" });
  assert.deepEqual(ttsResult, { path: "/tmp/speech.mp3", mime_type: "audio/mpeg", file_url: "file:///tmp/speech.mp3" });
  assert.deepEqual(imageResult, {
    ok: true,
    path: "/tmp/generated-image.png",
    mime_type: "image/png",
    thread_id: "thread-image",
    file_url: "file:///tmp/generated-image.png",
  });
  assert.deepEqual(persistedAudio, [{ base64: "Zm9v", mimeType: "audio/webm" }]);
  assert.deepEqual(queries, [
    [
      {
        type: "speech-to-text",
        args_json: JSON.stringify({
          provider: "openai",
          model: "whisper-1",
          language: "en",
          path: "/tmp/capture.webm",
          mime_type: "audio/webm",
        }),
      },
      "speech-to-text-result",
      30000,
    ],
    [
      {
        type: "text-to-speech",
        args_json: JSON.stringify({
          provider: "openai",
          model: "gpt-4o-mini-tts",
          response_format: "mp3",
          input: "Hello",
          voice: "alloy",
        }),
      },
      "text-to-speech-result",
      30000,
    ],
    [
      {
        type: "generate-image",
        args_json: JSON.stringify({
          thread_id: "thread-image",
          provider: "openai",
          model: "gpt-image-1",
          prompt: "retro robot portrait",
        }),
      },
      ["image-generation-result", "error"],
      120000,
    ],
  ]);
});

test("media generation failures come back as ok:false so the composer reports them", async () => {
  const handlers = new Map();
  registerAgentIpcHandlers(
    { handle: (name, handler) => handlers.set(name, handler) },
    {
      sendAgentCommand: () => {},
      sendAgentQuery: async () => {
        throw new Error("failed to execute generate_video: provider 'openai' does not support video generation");
      },
    },
    {
      logToFile: () => {},
      openAICodexAuthHandlers: {
        status: async () => ({ available: false }),
        login: async () => ({ available: false }),
        logout: async () => ({ ok: true }),
      },
      saveTempAudioCapture: () => ({ ok: true }),
    },
  );

  for (const channel of ["agent-generate-video", "agent-generate-image"]) {
    const result = await handlers.get(channel)({}, "anything", undefined);
    assert.equal(result.ok, false, `${channel} must flag the failure`);
    assert.match(result.error, /does not support video generation/);
  }
});
