declare module "epubjs" {
  export type Location = { start?: { cfi?: string } };
  export type Rendition = {
    display: (target?: string) => Promise<void>;
    next: () => Promise<void>;
    prev: () => Promise<void>;
    currentLocation: () => import("../features/reader/readerNavigation").PageLocation | undefined;
    destroy: () => void;
    on: (event: string, listener: (...args: unknown[]) => void) => void;
    getContents: () => Array<{ document: Document; cfiFromRange: (range: Range) => string }>;
    themes: {
      default: (rules: Record<string, Record<string, string>>) => void;
      fontSize: (size: string) => void;
    };
  };
  export type SpineSection = { href: string; next: () => SpineSection | undefined };
  export type Book = {
    locations: { generate: (characters: number) => Promise<string[]>; percentageFromCfi: (cfi: string) => number | null };
    loaded: { navigation: Promise<{ toc: import("../../packages/reader-core/ChapterPicker").Chapter[] }> };
    opened: Promise<void>;
    packaging: { metadata: { layout?: string } };
    renderTo: (element: HTMLElement, options: Record<string, unknown>) => Rendition;
    destroy: () => void;
    spine: { hooks: { content: { register: (callback: (document: Document) => void) => void } }; get: (target: string | number) => SpineSection | null; spineItems: Array<{ index: number; linear: string }> };
  };
  export default function ePub(input: string): Book;
}
