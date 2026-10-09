import { mount } from 'svelte';
import { mockIPC, mockWindows, mockConvertFileSrc } from '@tauri-apps/api/mocks';
import { DEFAULT_TITLE_STYLE, DEFAULT_BODY_STYLE } from './src/lib/types';
import './src/app.css';
const report = (e:any) => { document.getElementById('errors')!.textContent += String(e?.stack ?? e)+'\n'; };
window.addEventListener('error', e => report(e.error ?? e.message));
window.addEventListener('unhandledrejection', e => report(e.reason));
const main:any={id:'main',name:'Main',titleSize:72,bodySize:40,titleFont:'sans-serif',bodyFont:'sans-serif',textColor:'#ffffff',showBackground:true,showBody:true,textPosition:'center',titleStyle:{...DEFAULT_TITLE_STYLE},bodyStyle:{...DEFAULT_BODY_STYLE},positioning:'auto',titleBox:{x:5,y:10,width:90,height:20,zIndex:1},bodyBox:{x:5,y:35,width:90,height:45,zIndex:1},background:{type:'solid',color:'#123a5c'}};
const looks:any[]=[main,{...structuredClone(main),id:'stage',name:'Stage',showBackground:false}];
const slides:any[]=['song','scripture','generic'].map((kind,i)=>({id:'slide-'+i,itemId:'item-'+i,itemName:null,kind,name:['Verse 1','John 3:16','Welcome'][i],title:['Verse 1','John 3:16','Welcome'][i],body:['Amazing grace, how sweet the sound\nThat saved a wretch like me','For God so loved the world, that he gave his only begotten Son.','We are glad you are here'][i],background:{type:'solid',color:'#123a5c'},backgroundMode:'inherit',autoAdvanceSecs:null,libraryId:null,librarySlideId:null}));
const state:any={project:{schemaVersion:1,id:'repro',name:'Kind Look test',slides,looks,live:slides[0].id,selected:slides[0].id,showText:true,showBackground:true,aspectRatio:'16:9',transition:'cut',modifiedAt:'2026-10-06',itemLooks:{},itemBackgrounds:{}},items:slides.map((s,i)=>({id:s.itemId,name:['Test song','Test scripture','Welcome'][i],kind:s.kind,slideIds:[s.id]})),looks,outputLookId:'main',stageLookId:'stage',ndiLookId:null,defaultLooks:{song:null,scripture:null,generic:null},effectiveBackgrounds:{},effectiveItemLookIds:{},notice:null,firstRun:false,current:slides[0],next:slides[1],onDeck:slides[1],output:{visible:true,fullscreen:false,monitorIndex:null,monitorName:null},stage:{visible:false,monitorIndex:null,monitorName:null},broadcast:{enabled:false,available:false,state:'off',status:'off',sourceName:'Test',hasRealFrames:false,isStale:false},audio:{status:'stopped',currentPath:null,volume:1,deviceId:null,durationSecs:null,positionSecs:null},overlays:[],overlay:null,defaultTransition:'cut',midiEnabled:false,oscEnabled:false,oscPort:9000,triggers:[],stageNetworkEnabled:false,stageNetworkPort:9001,stageMessage:null,exitAnimation:null};
function snapshot(){for(const s of slides){const id=state.project.itemLooks[s.itemId]??state.defaultLooks[s.kind]??'main';state.effectiveItemLookIds[s.itemId]=id;state.effectiveBackgrounds[s.id]=looks.find(l=>l.id===id)?.background??s.background;}document.getElementById('trace')!.textContent=JSON.stringify(state);return JSON.parse(JSON.stringify(state));}
mockWindows('main');mockConvertFileSrc('linux');
mockIPC((cmd,args:any)=>{
 if(cmd==='get_state') return snapshot();
 if(cmd==='get_countdown') return {active:false,running:false,remainingSeconds:0,outputVisible:false,mode:null,targetTime:null};
 if(cmd==='get_library') return {schemaVersion:1,songs:[]};
 if(cmd==='get_bibles_folder') return '/tmp';
 if(cmd==='get_settings') return {defaultLooks:state.defaultLooks,triggers:[],audioVolume:1,oscPort:9000,stageNetworkPort:9001};
 if(cmd==='get_default_looks') return state.defaultLooks;
 if(cmd==='set_default_look'){state.defaultLooks[args.kind]=args.lookId;return snapshot();}
 if(cmd==='set_item_look'){if(args.lookId)state.project.itemLooks[args.itemId]=args.lookId;else delete state.project.itemLooks[args.itemId];return snapshot();}
 if(cmd==='set_selected'){state.project.selected=args.slideId;return snapshot();}
 if(cmd==='upsert_look'){Object.assign(looks.find(l=>l.id===args.lookId),JSON.parse(JSON.stringify(args.patch)));return snapshot();}
 if(cmd==='create_starter_looks'){
  for(const [name,kind] of [['Songs','song'],['Scripture','scripture'],['Text','generic'],['Title',null]]){
   const id=String(name).toLowerCase();if(!looks.some(l=>l.id===id)){const l={...structuredClone(main),id,name,showBody:name!=='Title'};if(name==='Songs'){l.titleSize=32;l.bodySize=72;l.bodyStyle.bold=true;}if(name==='Scripture'){l.titleSize=56;l.bodySize=44;l.titleStyle.bold=true;l.titleStyle.bgOpacity=.65;}if(name==='Text'){l.titleSize=96;l.bodySize=44;}if(name==='Title')l.titleSize=144;looks.push(l);}if(kind&&!state.defaultLooks[kind])state.defaultLooks[kind]=id;
  }return snapshot();
 }
 return [];
},{shouldMockEvents:true});
const {default:Editor}=await import('./src/components/Editor.svelte');mount(Editor,{target:document.getElementById('app')!});
