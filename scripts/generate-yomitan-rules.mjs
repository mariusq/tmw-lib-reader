// Generates offline Rust data from the pinned GPL-3.0-or-later Yomitan source.
// Copyright (C) 2026 TMW contributors. See vendor/yomitan/LICENSE.
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../crates/japanese-core/vendor/yomitan');
const factory=type=>(input,output,conditionsIn,conditionsOut)=>{
  if (/[.*+?^${}()|[\]\\]/u.test(input)) throw new Error(`Nonliteral rule requires explicit support: ${input}`);
  return {type,input,output,conditionsIn,conditionsOut};
};
const code=fs.readFileSync(path.join(root,'japanese-transforms.js'),'utf8')
  .replace(/^import .*;$/m,'').replace('export const japaneseTransforms =','const japaneseTransforms =');
const context=vm.createContext({suffixInflection:factory('suffix'),wholeWordInflection:factory('wholeWord')});
const descriptor=vm.runInContext(code+'\n;japaneseTransforms;',context,{timeout:1000});
const rules=Object.entries(descriptor.transforms).flatMap(([transform,value])=>value.rules.map(rule=>({...rule,transform})));
const output={sourceCommit:'77e200428902abf4fa48284df92da7af3dcb4162',license:'GPL-3.0-or-later',conditions:descriptor.conditions,rules};
fs.writeFileSync(path.join(root,'rules.json'),JSON.stringify(output,null,2)+'\n');
console.log(`Generated ${rules.length} rules and ${Object.keys(descriptor.conditions).length} conditions`);
