import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { RelayBlockSelection } from "./RelayBlockSelection";
afterEach(() => { window.getSelection()?.removeAllRanges(); cleanup(); });
it("reading links and copying text do not toggle the whole block", () => {
 const onChange=vi.fn();render(<RelayBlockSelection blocks={[{id:"one",kind:"PROSE",recommended:false,text:"完整正文。\n\n[查看证据](https://example.invalid/evidence)"}]} selected={[]} disabled={false} onChange={onChange}/>);
 const link=screen.getByRole("link");link.addEventListener("click",e=>e.preventDefault());fireEvent.click(link);expect(onChange).not.toHaveBeenCalled();
 const prose=screen.getByText("完整正文。");const range=document.createRange();range.selectNodeContents(prose);window.getSelection()?.addRange(range);fireEvent.click(prose);expect(onChange).not.toHaveBeenCalled();window.getSelection()?.removeAllRanges();fireEvent.click(prose);expect(onChange).toHaveBeenCalledWith(["one"]);
});
it("whole-block interaction is frozen when approval or pending work disables it",()=>{
 const onChange=vi.fn();render(<RelayBlockSelection blocks={[{id:"one",kind:"INSTRUCTION",recommended:true,text:"完整指令"}]} selected={["one"]} disabled onChange={onChange}/>);fireEvent.click(screen.getByText("完整指令"));expect(onChange).not.toHaveBeenCalled();expect(screen.getByRole("checkbox")).toBeDisabled();
});
