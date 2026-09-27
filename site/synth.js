// The AudioWorklet that plays koi's ambient sound: chunks of interleaved stereo that the page
// makes with Pond.sound and posts here, played in order. With none queued it plays silence,
// and it keeps at most 0.4 s queued, so a page that was held up does not fall behind.
class Synth extends AudioWorkletProcessor {
  constructor() {
    super();
    this.chunks = [];
    this.at = 0;
    this.port.onmessage = ({ data }) => {
      this.chunks.push(data);
      let queued = this.chunks.reduce((sum, chunk) => sum + chunk.length / 2, 0) - this.at / 2;
      while (queued > 0.4 * sampleRate && this.chunks.length > 1) {
        queued -= (this.chunks.shift().length - this.at) / 2;
        this.at = 0;
      }
    };
  }

  process(_inputs, [[left, right]]) {
    for (let i = 0; i < left.length && this.chunks.length > 0; i++) {
      const chunk = this.chunks[0];
      left[i] = chunk[this.at];
      right[i] = chunk[this.at + 1];
      this.at += 2;
      if (this.at >= chunk.length) {
        this.chunks.shift();
        this.at = 0;
      }
    }
    return true;
  }
}

registerProcessor("koi-synth", Synth);
