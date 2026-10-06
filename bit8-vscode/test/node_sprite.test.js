const assert = require('node:assert/strict');
const { test } = require('node:test');
const { nodePresentation } = require('../out/node_presentation');
const { parseSpriteInspection } = require('../out/project_model');
const { parseMapDocument, editMapNode } = require('../out/map_editor_model');
const { source } = require('./node_visual_fixture');
const sprites = [{ name:'Player', preview:'A1' },{ name:'Slime',preview:'A2' },{ name:'Empty',preview:null }];
const tiles = new Set(['A1','A2']);

for (const [name, values, expected] of [
  ['Sprite only',{sprite:'Player'},{source:'Sprite',tile:'A1'}],
  ['Sprite over Visual',{sprite:'Player',visual:'A2'},{source:'Sprite',tile:'A1'}],
  ['Sprite over static spr',{sprite:'Player',scriptVisual:'A2'},{source:'Sprite',tile:'A1'}],
  ['Sprite over both fallbacks',{sprite:'Player',visual:'A2',scriptVisual:'A2'},{source:'Sprite',tile:'A1'}],
  ['Visual fallback',{visual:'A2',scriptVisual:'A1'},{source:'Visual',tile:'A2'}],
  ['script fallback',{scriptVisual:'A2'},{source:'Script',tile:'A2'}],
  ['generic fallback',{}, {source:'Generic'}],
  ['empty Sprite stays authoritative',{sprite:'Empty',visual:'A1',scriptVisual:'A2'},{source:'Sprite'}],
  ['Camera special',{type:'Camera',sprite:'Player',visual:'A2'},{source:'Camera'}],
]) test(`Node presentation: ${name}`,()=>assert.deepEqual(nodePresentation({type:'Node',...values},sprites,tiles),expected));

test('core Sprite DTO is consumed without frontend preview inference',()=>{
  assert.deepEqual(parseSpriteInspection({sprites}),sprites);
  assert.deepEqual(parseSpriteInspection({}),[]);
  assert.throws(()=>parseSpriteInspection({sprites:[{name:'Bad',frames:['A2']}]}),/invalid/i);
});

test('read-only Sprite animation DTO preserves order, frames, FPS and loop with strict validation',()=>{
  const value={sprites:[{name:'Player',preview:'A4',animations:[
    {name:'walk',frames:['A4','A7','A5'],fps:8,loop:false},
    {name:'idle',frames:['A4'],fps:2,loop:true}
  ]}]};
  const before=JSON.stringify(value);
  assert.deepEqual(parseSpriteInspection(value),value.sprites);
  assert.equal(JSON.stringify(value),before);
  for(const patch of [{fps:31},{fps:0},{loop:'yes'},{frames:[]},{frames:['invalid']}]) {
    assert.throws(()=>parseSpriteInspection({sprites:[{...value.sprites[0],animations:[{...value.sprites[0].animations[0],...patch}]}]}),/invalid/i);
  }
});

test('Sprite edits survive reparse and clear without touching Visual/script/collider/other Node',()=>{
  const edit=(text,sprite)=>{const e=editMapNode(text,{type:'setSprite',id:'N1',sprite});return text.slice(0,e.startOffset)+e.replacement;};
  const original=parseMapDocument(source,tiles);
  const player=edit(source,'Player');
  assert.match(player,/sprite = "Player"/);
  assert.ok(player.indexOf('sprite =')<player.indexOf('[node_state.nodes.collider]'));
  const reopened=parseMapDocument(player,tiles);
  assert.equal(reopened.nodes[0].sprite,'Player');
  assert.deepEqual(reopened.nodes[0].collider,original.nodes[0].collider);
  assert.equal(reopened.nodes[0].script,original.nodes[0].script);
  assert.deepEqual(reopened.nodes[1],original.nodes[1]);
  assert.deepEqual(reopened.cells,original.cells);
  const slime=edit(player,'Slime');
  assert.equal(parseMapDocument(slime,tiles).nodes[0].sprite,'Slime');
  assert.equal((slime.match(/^sprite = /gm)||[]).length,1);
  assert.equal(edit(slime,null),source);
});
