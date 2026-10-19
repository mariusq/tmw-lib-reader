import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import Catalog from "./Catalog";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./SavedPassages", () => ({ default: () => null }));
class Observer { observe() {} disconnect() {} }
vi.stubGlobal("IntersectionObserver", Observer);
vi.stubGlobal("scrollTo", vi.fn());
afterEach(() => { localStorage.clear(); history.replaceState(null, ""); vi.resetAllMocks(); });

it("shows synced PC reading before download and offers the private download action", async () => {
  localStorage.setItem("tmw-show-covers", "false");
  vi.mocked(invoke).mockImplementation(async (_, payload) => {
    const args=(payload as {args:Record<string,unknown>}).args;
    if(args.action==="browse")return {items:args.recent ? [{ns:"pc",id:"remote",title:"PC story",creator:"Author",bytes:10,available:true,deleted:false}] : [],ns:"pc",namespaces:["pc"],next:null};
    if(args.action==="cacheSettings")return {budget:100000000,usage:0};
    return {};
  });
  render(<Catalog onOpen={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button",{name:"Download to resume: PC story"}));
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("mobile_storage",{args:{action:"download",ns:"pc",id:"remote"}}));
});

it("opens offline series, expands copies and restores the search on Android Back", async () => {
  history.replaceState({ tmwTab: "library" }, "");
  localStorage.setItem("tmw-show-covers", "false");
  localStorage.setItem("tmw-consolidate-series", "true");
  localStorage.setItem("tmw-series-confidence", "medium");
  const base = { ns:"pc",title:"Story 1",creator:"Author",series:"",volume:"1",bytes:10,available:true,deleted:false };
  vi.mocked(invoke).mockImplementation(async (command,payload) => {
    if(command==="read_mobile_book")return new ArrayBuffer(10);
    const args=(payload as {args:Record<string,unknown>}).args;
    if(args.action==="cacheSettings")return {budget:100000000,usage:0};
    if(args.action!=="browse")return {};
    const page={ns:"pc",namespaces:["pc"],next:null,seriesReady:true};
    if(args.recent)return {...page,items:[]};
    if(args.selectedCopy)return {...page,items:[{...base,id:"copy2",download:{file:"copy2.epub",version:"sha256-version",bytes:10}}]};
    if(args.selectedSeries)return {...page,items:[{...base,id:"copy1",copyGroup:{key:"copies",count:2}}]};
    return {...page,items:[{...base,id:"copy1",seriesCard:{key:"proposal:fixture",name:"Story",count:3}}]};
  });
  const onOpen=vi.fn();render(<Catalog onOpen={onOpen} />);
  fireEvent.change(screen.getByLabelText("Search local catalog"),{target:{value:"tokidoki"}});
  fireEvent.click(await screen.findByRole("button",{name:/View series/}));
  expect(await screen.findByRole("navigation",{name:"Breadcrumb"})).toBeTruthy();
  await screen.findByText("Show 2 copies");
  const detail=screen.getByText("Show 2 copies").closest("details")!;
  detail.open=true;fireEvent(detail,new Event("toggle"));
  fireEvent.click(await screen.findByRole("button",{name:"Read offline"}));
  await vi.waitFor(() => expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({file:"copy2.epub",catalog:expect.objectContaining({id:"copy2"})})));
  expect(invoke).toHaveBeenCalledWith("mobile_storage",{args:expect.objectContaining({selectedSeries:"proposal:fixture",selectedCopy:"copies",seriesConfidence:"medium",groupCopies:false})});
  fireEvent.click(screen.getByRole("button",{name:"← Back to library"}));
  await vi.waitFor(() => expect((screen.getByLabelText("Search local catalog") as HTMLInputElement).value).toBe("tokidoki"));
  expect(await screen.findByRole("button",{name:/View series/})).toBeTruthy();
});
it("shows recent downloads independently of browsing and opens their current local copy", async () => {
  localStorage.setItem("tmw-show-covers", "false");
  const books = Array.from({ length: 5 }, (_, i) => ({ ns: "pc", id: `id${i}`, title: `Story ${i}`, creator: "Author", download: { file: `book${i}.epub`, version: "sha256-version", bytes: 10 } }));
  vi.mocked(invoke).mockImplementation(async (command, payload) => {
    if (command === "read_mobile_book") return new ArrayBuffer(10);
    const args = (payload as { args: { action: string; recent?: boolean } }).args;
    if (args.action === "browse") return { items: args.recent ? books : [], ns: "pc", namespaces: ["pc"], next: null };
    if (args.action === "cacheSettings") return { budget: 100000000, usage: 0 };
    return {};
  });
  const onOpen = vi.fn();
  render(<Catalog onOpen={onOpen} />);
  expect(screen.getByRole("heading", { name: "Jump back in" })).toBeTruthy();
  fireEvent.click(await screen.findByRole("button", { name: "Resume: Story 0" }));
  await vi.waitFor(() => expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ file: "book0.epub", catalog: { ns: "pc", id: "id0", version: "sha256-version" } })));
  expect(screen.getAllByRole("button", { name: /^Resume:/ })).toHaveLength(5);
  expect(invoke).not.toHaveBeenCalledWith("mobile_storage", { args: expect.objectContaining({ action: "cover" }) });
});
