import JSZip from "jszip";

// Archive parsing and inflation stay off the WebView thread. Requests are serialized
// so resource replacements cannot inflate an entire large publication concurrently.
let zip: JSZip | undefined;
let queue = Promise.resolve();
self.onmessage = (event: MessageEvent) => {
  const { id, bytes, name, type } = event.data;
  queue = queue.then(async () => {
    try {
      let value: unknown;
      if (bytes) {
        zip = await JSZip.loadAsync(bytes);
        let total = 0;
        const files = Object.values(zip.files).filter((f) => !f.dir);
        if (files.length > 10000) throw new Error("Too many EPUB entries.");
        for (const file of files) {
          const size = (file as unknown as { _data?: { uncompressedSize?: number } })._data
            ?.uncompressedSize;
          if (size === undefined || size > 16_000_000)
            throw new Error("EPUB entry exceeds the 16 MB reader limit.");
          total += size;
          if (total > 128_000_000)
            throw new Error("EPUB exceeds the 128 MB expanded reader limit.");
        }
        value = files.map((f) => f.name);
      } else {
        const entry = zip?.file(name);
        if (!entry) throw new Error("EPUB resource is missing.");
        value = await entry.async(type);
      }
      if (value instanceof Uint8Array)
        self.postMessage({ id, value }, { transfer: [value.buffer] });
      else self.postMessage({ id, value });
    } catch (error) {
      self.postMessage({ id, error: String(error) });
    }
  });
};
