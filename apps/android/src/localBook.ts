export type LocalBook = {
  name: string;
  file?: string;
  bytes: ArrayBuffer;
  id?: string;
  catalog?: { ns: string; id: string; version: string };
};
function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("tmw-offline-reader", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("books");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
export async function localBook(value?: LocalBook): Promise<LocalBook | undefined> {
  const db = await open();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("books", value ? "readwrite" : "readonly");
    const store = tx.objectStore("books");
    const request = value ? store.put(value, "current") : store.get("current");
    tx.oncomplete = () => {
      db.close();
      resolve(value ?? request.result);
    };
    tx.onerror = () => {
      db.close();
      reject(tx.error);
    };
    tx.onabort = () => {
      db.close();
      reject(tx.error ?? new Error("Local copy was not saved"));
    };
  });
}
