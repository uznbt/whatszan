import sharp from "sharp";
import { readFileSync, writeFileSync } from "fs";

async function test() {
  const buffer = readFileSync("static/app.png");
  const badgeSvg = Buffer.from(
    `<svg width="120" height="120">
       <circle cx="60" cy="60" r="50" fill="#FF0000" stroke="#FFFFFF" stroke-width="10"/>
     </svg>`
  );
  const out = await sharp(buffer)
    .composite([{ input: badgeSvg, top: 90, left: 300 }])
    .png()
    .toBuffer();
  writeFileSync("static/app-badged.png", out);
  console.log("Done");
}

test();
