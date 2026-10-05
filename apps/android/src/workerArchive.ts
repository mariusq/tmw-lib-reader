import ePub, { type Book } from "epubjs";
import Archive from "epubjs/lib/archive";

/** Adapter for the audited epub.js 0.3 archive interface; no global patches. */
export function createReaderBook(bytes: ArrayBuffer): Book {
  // The DOM test environment has no workers. Production Android requires them.
  if (typeof Worker === "undefined") return ePub(bytes);
  const worker = new Worker(new URL("./epubArchive.worker.ts", import.meta.url), {
    type: "module",
  });
  let sequence = 0;
  const pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (e: Error) => void }
  >();
  let ended = false;
  const fail = (error: Error) => {
    for (const request of pending.values()) request.reject(error);
    pending.clear();
  };
  worker.onerror = () => fail(new Error("EPUB worker failed. Reopen the book to retry."));
  worker.onmessage = (event) => {
    const request = pending.get(event.data.id);
    if (!request) return;
    pending.delete(event.data.id);
    if (event.data.error) request.reject(new Error(event.data.error));
    else request.resolve(event.data.value);
  };
  const request = (args: Record<string, unknown>, transfer: Transferable[] = []) =>
    new Promise<unknown>((resolve, reject) => {
      if (ended) {
        reject(new Error("Reader closed."));
        return;
      }
      const id = ++sequence;
      pending.set(id, { resolve, reject });
      worker.postMessage({ id, ...args }, transfer);
    });
  const archive = new Archive();
  let entries = new Set<string>();
  // Keep the selected copy intact for reopening; transfer only the worker-owned copy.
  const input = bytes.slice(0);
  archive.zip = {
    loadAsync: async () => {
      entries = new Set((await request({ bytes: input }, [input])) as string[]);
    },
    file: (name: string) =>
      entries.has(name) ? { name, async: (type: string) => request({ name, type }) } : null,
  };
  // epub.js's request/createUrl wrappers otherwise fail to propagate inflation errors.
  archive.request = async (url: string, type?: string) => {
    const extension = type ?? url.split(".").pop() ?? "";
    const response = extension === "blob" ? archive.getBlob(url) : archive.getText(url);
    if (!response) throw new Error("EPUB resource is missing.");
    return archive.handleResponse(await response, extension);
  };
  archive.createUrl = async (url: string, options?: { base64?: boolean }) => {
    if (archive.urlCache[url]) return archive.urlCache[url];
    const response = options?.base64 ? archive.getBase64(url) : archive.getBlob(url);
    if (!response) throw new Error("EPUB resource is missing.");
    const value = await response;
    return (archive.urlCache[url] = typeof value === "string" ? value : URL.createObjectURL(value));
  };
  const book = ePub();
  (book as unknown as { unarchive: () => Promise<unknown> }).unarchive = async () => {
    (book as unknown as { archive: typeof archive }).archive = archive;
    await archive.open(input);
  };
  const destroy = book.destroy.bind(book);
  book.destroy = () => {
    ended = true;
    worker.terminate();
    fail(new Error("Reader closed."));
    archive.destroy();
    destroy();
  };
  void book.open(bytes, "binary").catch(() => {
    /* surfaced through book.opened */
  });
  return book;
}
