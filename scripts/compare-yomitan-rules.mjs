// GPL-3.0-or-later: executes pinned Yomitan sources to create comparison evidence.
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../crates/japanese-core/vendor/yomitan');
const load=name=>fs.readFileSync(path.join(root,name),'utf8').replace(/^import .*;$/gm,'').replace(/export (function|class|const) /g,'$1 ');
const samples=[
 ['死んじゃえ','死ぬ','v5'],['信じらんない','信じる','v1'],['信っじらんない','信じる','v1'],
 ['読んじゃった','読む','v5'],['食べさせられました','食べる','v1'],['食べている','食べる','v1'],
 ['高くなかった','高い','adj-i'],['読まなかった','読む','v5'],['行った','行く','v5'],
 ['来ました','来る','vk'],['勉強しました','勉強する','vs'],['書け','書く','v5'],
 ['食べろ','食べる','v1'],['読んどいた','読む','v5'],['食べちゃえ','食べる','v1'],
 ['わかんない','わかる','v5'],['食べらんない','食べる','v1'],['高ければ','高い','adj-i'],
 ['読めば','読む','v5'],['読もう','読む','v5'],['食べよう','食べる','v1'],
 ['読まれた','読む','v5'],['読ませた','読む','v5'],['読める','読む','v5'],
 ['食べませんでした','食べる','v1'],['読んでしまった','読む','v5'],['せん','する','vs'],
 ['持ってこい','持ってくる','vk'],['しなきゃ','する','vs'],['食べへん','食べる','v1']
];
const context=vm.createContext({log:{warn:()=>{}}});
vm.runInContext(load('language-transforms.js')+'\n'+load('japanese-transforms.js')+'\n'+load('language-transformer.js')+'\n;globalThis.oracle=new LanguageTransformer(); oracle.addDescriptor(japaneseTransforms);',context,{timeout:1000});
context.samples=samples;
const evidence=vm.runInContext(`samples.map(([text,term,pos])=>{const flags=oracle.getConditionFlagsFromPartsOfSpeech([pos]);const results=oracle.transform(text);const matches=results.filter(r=>r.text===term&&LanguageTransformer.conditionsMatch(r.conditions,flags)); return {text,term,pos,matched:matches.length>0,depth:matches.length?Math.min(...matches.map(r=>r.trace.length)):null};})`,context,{timeout:10000});
fs.writeFileSync(path.join(root,'comparison.json'),JSON.stringify({sourceCommit:'77e200428902abf4fa48284df92da7af3dcb4162',cases:evidence},null,2)+'\n');
console.log(JSON.stringify(evidence));
