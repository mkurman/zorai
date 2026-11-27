import { base64ToPcm16, bytesToBase64, floatToPcm16, pcm16ToFloat } from "./realtimeEvents";

const CAPTURE_WORKLET = `class ZoraiCapture extends AudioWorkletProcessor {
  constructor() { super(); this.buffer = []; this.size = 0; }
  process(inputs) {
    const channel = inputs[0] && inputs[0][0];
    if (channel) { this.buffer.push(channel.slice(0)); this.size += channel.length; }
    if (this.size >= sampleRate / 10) {
      const out = new Float32Array(this.size);
      let offset = 0;
      for (const part of this.buffer) { out.set(part, offset); offset += part.length; }
      this.port.postMessage(out, [out.buffer]);
      this.buffer = []; this.size = 0;
    }
    return true;
  }
}
registerProcessor("zorai-capture", ZoraiCapture);`;

export async function createPcmCapture(context: AudioContext, mic: MediaStream, onChunk: (base64Pcm16: string) => void): Promise<() => void> {
  const workletUrl = URL.createObjectURL(new Blob([CAPTURE_WORKLET], { type: "text/javascript" }));
  await context.audioWorklet.addModule(workletUrl);
  URL.revokeObjectURL(workletUrl);
  const capture = new AudioWorkletNode(context, "zorai-capture");
  const source = context.createMediaStreamSource(mic);
  const silent = context.createGain();
  silent.gain.value = 0;
  source.connect(capture);
  capture.connect(silent);
  silent.connect(context.destination);
  capture.port.onmessage = (message) => {
    if (!mic.getAudioTracks().some((track) => track.enabled)) return;
    const pcm = floatToPcm16(message.data as Float32Array);
    onChunk(bytesToBase64(new Uint8Array(pcm.buffer)));
  };
  return () => {
    capture.port.onmessage = null;
    source.disconnect();
    capture.disconnect();
    silent.disconnect();
  };
}

export function createPcmPlayer(context: AudioContext, rate: number) {
  const playing = new Set<AudioBufferSourceNode>();
  let playhead = 0;
  let responseStart: number | null = null;
  const interrupt = () => {
    for (const node of playing) { try { node.stop(); } catch { continue; } }
    playing.clear();
    playhead = 0;
    responseStart = null;
  };
  const startResponse = () => { responseStart = null; };
  const progress = () => {
    if (responseStart === null) return undefined;
    const total = playhead - responseStart;
    return total > 0 ? Math.max(0, Math.min(1, (context.currentTime - responseStart) / total)) : undefined;
  };
  const play = (base64: string) => {
    if (!base64) return;
    const samples = pcm16ToFloat(base64ToPcm16(base64));
    if (!samples.length) return;
    const buffer = context.createBuffer(1, samples.length, rate);
    buffer.copyToChannel(samples, 0);
    const node = context.createBufferSource();
    node.buffer = buffer;
    node.connect(context.destination);
    playhead = Math.max(playhead, context.currentTime + 0.03);
    if (responseStart === null) responseStart = playhead;
    node.start(playhead);
    playhead += buffer.duration;
    playing.add(node);
    node.onended = () => playing.delete(node);
  };
  return { play, interrupt, startResponse, progress, isPlaying: () => playing.size > 0 };
}
