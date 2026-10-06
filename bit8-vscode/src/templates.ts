export interface Bit8CodeTemplate {
  readonly label: string;
  readonly trigger: string;
  readonly description: string;
  readonly source: string;
}

export const bit8CodeTemplates: readonly Bit8CodeTemplate[] = [
  {
    label: "Bit8 Template: Basic Game",
    trigger: "Template",
    description: "A small movable-pixel game using the current Bit8 API.",
    source: [
      "func init()",
      "    playerX = 32",
      "end",
      "",
      "func update()",
      "    if btn(LEFT) and playerX > 0 then",
      "        playerX = playerX - 1",
      "    end",
      "    if btn(RIGHT) and playerX < 63 then",
      "        playerX = playerX + 1",
      "    end",
      "end",
      "",
      "func draw()",
      "    cls(0)",
      "    pix(playerX, 32, 1)",
      "end",
    ].join("\n"),
  },
];
