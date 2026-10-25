import {createRoot} from 'react-dom/client';
import {CacheSettings} from '../../src/features/workbench/CacheSettings';
import {UrlBoardFrame} from '../../src/features/character-board/UrlBoardFrame';
import {boardCache,BoardPersistentCache} from '../../src/features/character-board/boardCache';
import {markSessionLive} from '../../src/mobile/startup';
import '../../src/styles/pwa-update.css';import '../../src/features/character-board/board-hub.css';
boardCache.setScope('cache-maintenance-qa');markSessionLive(true);Object.assign(window,{cacheQA:{boardCache,BoardPersistentCache}});
const board=new URLSearchParams(location.search).has('board');
createRoot(document.getElementById('root')!).render(board?<div style={{height:'100dvh'}}><UrlBoardFrame src={location.origin+'/tests/character-board/url-board.html'} active/></div>:<main style={{fontFamily:'system-ui',maxWidth:600,margin:'24px auto',padding:20}}><h1 style={{fontSize:22}}>关于 Agbrio</h1><CacheSettings/></main>);
