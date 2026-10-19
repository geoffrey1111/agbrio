import {expect,it} from 'vitest';
import {validReviewRequest} from './reviewProtocol';
it('permits only the exact bounded review vocabulary',()=>{
 for(const [method,route] of [['GET','/api/reviews'],['POST','/api/draft'],['POST','/api/submit'],['POST','/api/notify/00000000-0000-4000-8000-000000000000']])expect(validReviewRequest({method,route,body:null})).toBe(true);
 for(const [method,route] of [['POST','http://127.0.0.1:47816/api/submit'],['GET','/api/reviews?x=1'],['DELETE','/api/draft'],['POST','/api/notify/../../draft'],['POST','/api/%73ubmit']])expect(validReviewRequest({method,route})).toBe(false);
 expect(validReviewRequest({method:'POST',route:'/api/submit',revision:'override'})).toBe(false);
 expect(validReviewRequest({method:'POST',route:'/api/draft',body:'中'.repeat(30000)})).toBe(false);
});
