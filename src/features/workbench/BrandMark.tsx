import mark from "../../assets/brand/mark.png";

export function BrandMark({size=32}:{size?:number}){
 return <img src={mark} alt="" aria-hidden="true" width={size} height={size} style={{display:"block",flexShrink:0,borderRadius:Math.round(size*.22)}}/>;
}
