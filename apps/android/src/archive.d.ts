declare module "epubjs/lib/archive" {
  export default class Archive {
    zip: unknown;
    urlCache: Record<string, string>;
    open(bytes: ArrayBuffer): Promise<unknown>;
    request(url: string, type?: string): Promise<unknown>;
    handleResponse(value: unknown, type: string): unknown;
    getText(url: string): Promise<string> | undefined;
    getBlob(url: string): Promise<Blob> | undefined;
    getBase64(url: string): Promise<string> | undefined;
    createUrl(url: string, options?: { base64?: boolean }): Promise<string>;
    destroy(): void;
  }
}
