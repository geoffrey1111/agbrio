import {writeFile,rename} from 'node:fs/promises';
const transient=new Set(['EPERM','EACCES','EBUSY']);
export async function replaceStateFile(path,value,{write=writeFile,move=rename,pause=ms=>new Promise(yes=>setTimeout(yes,ms)),attempts=21,delayMs=100}={}){
 // Windows readers/AV can briefly deny delete-sharing of an existing file.
 // Keep atomic replacement; never truncate the last committed state or unlink
 // it to make the rename succeed. The pending snapshot remains on final failure.
 const pending=path+'.pending';await write(pending,JSON.stringify(value));
 for(let attempt=0;;attempt++){
  try{await move(pending,path);return;}
  catch(error){if(!transient.has(error.code)||attempt+1>=attempts)throw error;await pause(delayMs);}
 }
}
