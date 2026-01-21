import{a as S}from"./chunk-OCKYRLHM.js";import{Ba as s,Fa as m,La as d,Ma as e,Na as t,Wa as n,Xa as a,Ya as v,ra as i,sb as x,vb as f,xa as p}from"./chunk-UNNPGNX6.js";function u(o,c){if(o&1&&(e(0,"article",9)(1,"h3"),n(2),t(),e(3,"p"),n(4),t()()),o&2){let r=c.$implicit;i(2),a(r.title),i(2),a(r.text)}}function E(o,c){if(o&1&&(e(0,"article",10)(1,"h3"),n(2),t(),e(3,"pre")(4,"code"),n(5),t()()()),o&2){let r=c.$implicit;i(2),a(r.title),i(3),a(r.code)}}function _(o,c){if(o&1&&(e(0,"article",9)(1,"h3"),n(2),t(),e(3,"p"),n(4),t()()),o&2){let r=c.$implicit;i(2),a(r.title),i(2),a(r.text)}}function I(o,c){if(o&1&&(e(0,"article",9)(1,"p"),n(2),t()()),o&2){let r=c.$implicit;i(2),a(r)}}function h(o,c){if(o&1&&(e(0,"article",9)(1,"p"),n(2),t()()),o&2){let r=c.$implicit;i(2),a(r)}}function b(o,c){if(o&1&&(e(0,"article",10)(1,"pre")(2,"code"),n(3),t()()()),o&2){let r=c.$implicit;i(3),v("steel@mymachine$ ",r)}}var g=class o{constructor(c){this.state=c}get t(){return this.state.t()}get examples(){return this.state.examples}static \u0275fac=function(r){return new(r||o)(p(S))};static \u0275cmp=s({type:o,selectors:[["app-docs"]],decls:58,vars:21,consts:[[1,"section"],[1,"section-head"],[1,"eyebrow"],[1,"section-lead"],[1,"card-grid","three"],["class","panel",4,"ngFor","ngForOf"],[1,"panel","terminal","wide"],["class","panel terminal",4,"ngFor","ngForOf"],[1,"card-grid","two"],[1,"panel"],[1,"panel","terminal"]],template:function(r,l){r&1&&(e(0,"section",0)(1,"div",1)(2,"p",2),n(3),t(),e(4,"h2"),n(5),t(),e(6,"p",3),n(7),t()(),e(8,"div",4),m(9,u,5,2,"article",5),t(),e(10,"article",6)(11,"h3"),n(12),t(),e(13,"pre")(14,"code"),n(15,`!muf 4

[workspace]
	.set name "demo"
	.set root "."
	.set target_dir "target"
	.set profile "debug"
..

[tool cc]
	.exec "cc"
..

[bake build]
	.make c_src cglob "src/**/*.c"
	[run cc]
		.takes c_src as "@args"
		.set "-O2" 1
		.set "-g" 1
		.emits exe as "-o"
	..
	.output exe "target/out/app"
..`),t()()()(),e(16,"section",0)(17,"div",1)(18,"p",2),n(19),t(),e(20,"h2"),n(21),t(),e(22,"p",3),n(23),t()(),e(24,"div",4),m(25,E,6,2,"article",7),t()(),e(26,"section",0)(27,"div",1)(28,"p",2),n(29),t(),e(30,"h2"),n(31),t()(),e(32,"div",4),m(33,_,5,2,"article",5),t()(),e(34,"section",0)(35,"div",1)(36,"p",2),n(37),t(),e(38,"h2"),n(39),t()(),e(40,"div",8),m(41,I,3,1,"article",5),t()(),e(42,"section",0)(43,"div",1)(44,"p",2),n(45),t(),e(46,"h2"),n(47),t()(),e(48,"div",8),m(49,h,3,1,"article",5),t()(),e(50,"section",0)(51,"div",1)(52,"p",2),n(53),t(),e(54,"h2"),n(55),t()(),e(56,"div",8),m(57,b,4,1,"article",7),t()()),r&2&&(i(3),a(l.t.docs.eyebrow),i(2),a(l.t.docs.title),i(2),a(l.t.docs.lead),i(2),d("ngForOf",l.t.docs.sections),i(3),a(l.t.examples.minimalTitle),i(7),a(l.t.examples.eyebrow),i(2),a(l.t.examples.title),i(2),a(l.t.examples.lead),i(2),d("ngForOf",l.examples),i(4),a(l.t.docs.guidesTitle),i(2),a(l.t.docs.guidesTitle),i(2),d("ngForOf",l.t.docs.guides),i(4),a(l.t.docs.troubleshootingTitle),i(2),a(l.t.docs.troubleshootingTitle),i(2),d("ngForOf",l.t.docs.troubleshooting),i(4),a(l.t.docs.bestPracticesTitle),i(2),a(l.t.docs.bestPracticesTitle),i(2),d("ngForOf",l.t.docs.bestPractices),i(4),a(l.t.docs.commandsTitle),i(2),a(l.t.docs.commandsTitle),i(2),d("ngForOf",l.t.docs.commandList))},dependencies:[f,x],encapsulation:2})};export{g as DocsComponent};
