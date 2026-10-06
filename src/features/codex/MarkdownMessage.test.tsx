import { describe, expect, it, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor, within } from "@testing-library/react";
import { MarkdownMessage } from "./MarkdownMessage";

describe("MarkdownMessage", () => {
  it("renders raw HTML as text rather than executable markup", () => {
    const { container } = render(<MarkdownMessage text={'<img src=x onerror="window.__pwned=true">'} />);
    expect(container.querySelector("img")).toBeNull();
    expect(container).toHaveTextContent("<img src=x");
    expect((window as Window & { __pwned?: boolean }).__pwned).toBeUndefined();
  });

  it("renders owner-requested remote preview without exposing referrer or credentials", () => {
    const { container } = render(<MarkdownMessage text="![diagram](https://example.invalid/diagram.png)" />);
    expect(screen.getByRole("img", { name: "diagram" })).toHaveAttribute("src","https://example.invalid/diagram.png");
    expect(container.querySelector("img")).toHaveAttribute("crossorigin","anonymous");
  });
});

afterEach(cleanup);
it("renders clickable markdown and bare HTTPS links and remote image preview",()=>{render(<MarkdownMessage text={"[报告](https://example.invalid/report)\n\nhttps://example.invalid/plain\n\n![结果图](https://example.invalid/image.png)"}/>);expect(screen.getByRole("link",{name:"报告"})).toHaveAttribute("href","https://example.invalid/report");expect(screen.getByRole("link",{name:"https://example.invalid/plain"})).toBeInTheDocument();expect(screen.getByRole("img",{name:"结果图"})).toHaveAttribute("src","https://example.invalid/image.png");expect(screen.getByRole("img",{name:"结果图"})).toHaveAttribute("referrerpolicy","no-referrer");});
it("resolves a scoped local image, keeps failed media visible, and opens a viewer",async()=>{const media=vi.fn().mockResolvedValue({filename:"image.png",mime:"image/png",data:"fixture"});render(<MarkdownMessage text="![本地图](image.png)" media={media}/>);await screen.findByRole("img",{name:"本地图"});expect(media).toHaveBeenCalledWith("image.png");fireEvent.click(screen.getByRole("button",{name:"查看图片：本地图"}));expect(screen.getByRole("dialog",{name:"本地图"})).toBeInTheDocument();});
it("does not execute raw HTML or javascript links and exposes missing images",async()=>{render(<MarkdownMessage text={'<script>alert(1)</script>\n\n[bad](javascript:alert)\n\n![missing](missing.png)'}/>);await waitFor(()=>expect(screen.getByText(/图片暂不可用/)).toBeInTheDocument());expect(document.querySelector('script')).toBeNull();expect(screen.getByRole("link",{name:"bad"})).not.toHaveAttribute("href","javascript:alert");});

it("keeps an open image viewer and its bytes across progress text refreshes",async()=>{const media=vi.fn().mockResolvedValue({filename:"image.png",mime:"image/png",data:"fixture"});const view=render(<MarkdownMessage text="![原图](image.png)" media={media}/>);await screen.findByRole("img",{name:"原图"});fireEvent.click(screen.getByRole("button",{name:"查看图片：原图"}));view.rerender(<MarkdownMessage text="![原图](image.png)\n\n进度已更新。" media={media}/>);expect(screen.getByRole("dialog",{name:"原图"})).toBeInTheDocument();expect(media).toHaveBeenCalledTimes(1);});

it("retains the opened image snapshot when a later reply removes the thumbnail",async()=>{const media=vi.fn().mockResolvedValue({filename:"image.png",mime:"image/png",data:"fixture"});const view=render(<MarkdownMessage text="![原图](image.png)" media={media}/>);await screen.findByRole("img",{name:"原图"});fireEvent.click(screen.getByRole("button",{name:"查看图片：原图"}));fireEvent.click(screen.getByRole("button",{name:"放大图片"}));view.rerender(<MarkdownMessage text="新回复已到达。" media={media}/>);expect(screen.getByRole("dialog",{name:"原图"})).toBeInTheDocument();expect(screen.getByRole("img",{name:"原图"})).toHaveAttribute("src","data:image/png;base64,fixture");fireEvent.click(screen.getByRole("button",{name:"关闭图片"}));expect(screen.queryByRole("dialog")).not.toBeInTheDocument();});
it("copies only code bytes and reports an unavailable clipboard without throwing",async()=>{const original=navigator.clipboard;Object.defineProperty(navigator,"clipboard",{configurable:true,value:undefined});render(<MarkdownMessage text={'```text\nconst owner = "$literal";\n```'}/>);fireEvent.click(screen.getByRole("button",{name:"复制代码"}));expect(await screen.findByRole("status")).toHaveTextContent("复制未成功");cleanup();const writeText=vi.fn().mockResolvedValue(undefined);Object.defineProperty(navigator,"clipboard",{configurable:true,value:{writeText}});render(<MarkdownMessage text={'```text\nconst owner = "$literal";\n```'}/>);fireEvent.click(screen.getByRole("button",{name:"复制代码"}));await screen.findByRole("button",{name:"已复制代码"});expect(writeText).toHaveBeenCalledWith('const owner = "$literal";\n');Object.defineProperty(navigator,"clipboard",{configurable:true,value:original});});

it("magnifies a small image beyond its intrinsic size and restores its fit",async()=>{render(<MarkdownMessage text="![small](https://example.invalid/image.png)"/>);fireEvent.click(screen.getByRole("button",{name:"查看图片：small"}));const img=within(screen.getByRole("dialog")).getByRole("img");Object.defineProperty(img,"naturalWidth",{value:100});Object.defineProperty(img,"naturalHeight",{value:50});fireEvent.load(img);expect(img).toHaveStyle({width:"100px",height:"50px"});fireEvent.click(screen.getByRole("button",{name:"放大图片"}));expect(img).toHaveStyle({width:"150px",height:"75px"});fireEvent.click(screen.getByRole("button",{name:"适应屏幕"}));expect(img).toHaveStyle({width:"100px",height:"50px"});});
