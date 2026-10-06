const assert = require("node:assert/strict");
const { test } = require("node:test");
const { staticNodeSprite } = require("../out/node_script_visual");

test("static Node draw hints recognize only one direct stable sprite at self.x/self.y", () => {
  for (const [source, id] of [
    ["func draw()\n spr(A4, self.x, self.y)\nend", "A4"],
    ["function draw() sprite(B12,self.x,self.y) end", "B12"],
    ["-- example\nfunc draw ( )\n spr (\n C3 , self . x , self . y\n ); -- comment\nend", "C3"],
    ["func update() if btn(A) then self.x = 1 end end\nfunc draw() spr(A7,self.x,self.y) end", "A7"],
    ["--[=[ func draw() spr(A99,self.x,self.y) end ]=]\nfunc draw() spr(A4,self.x,self.y) end", "A4"],
  ]) assert.equal(staticNodeSprite(source), id, source);
});

test("static hints reject expressions, control flow, multiple calls and quoted/commented fake code", () => {
  for (const source of [
    "func draw() spr(self.frame,self.x,self.y) end",
    "func draw() spr(A4,self.x+1,self.y) end",
    "func draw() if self.walking then spr(A5,self.x,self.y) else spr(A4,self.x,self.y) end end",
    "func draw() spr(A4,self.x,self.y) spr(A5,self.x,self.y) end",
    "func draw() other() spr(A4,self.x,self.y) end",
    "func draw() for i=1,2 do spr(A4,self.x,self.y) end end",
    "if flag then func draw() spr(A4,self.x,self.y) end end",
    "local function draw() spr(A4,self.x,self.y) end",
    "func draw() spr(A4,self.x,self.y) end\nfunc draw() spr(A5,self.x,self.y) end",
    'text = "func draw() spr(A4,self.x,self.y) end"',
    "-- func draw() spr(A4,self.x,self.y) end",
    "text = [[func draw() spr(A4,self.x,self.y) end]]",
    "func draw() spr(A4,self.x,self.y) end\ndraw = other",
    "func draw() spr(A0,self.x,self.y) end",
    "func draw() spr(A4,self.x,self.y)",
    "--[[ unterminated\nfunc draw() spr(A4,self.x,self.y) end",
    "", 
  ]) assert.equal(staticNodeSprite(source), undefined, source);
});
