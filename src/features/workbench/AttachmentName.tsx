export function AttachmentName({name}:{name:string}){
 const dot=name.lastIndexOf('.');
 const split=dot>0&&/^[a-z0-9]{1,16}$/i.test(name.slice(dot+1));
 return <span className="v4-file-name" title={name}><span className="v4-file-stem">{split?name.slice(0,dot):name}</span>{split&&<span className="v4-file-extension">{name.slice(dot)}</span>}</span>;
}
