const vals = ['undefined','null','true','false','0','-0','1','2','1.5','1e400x','""','"1"','"10"','"9"','" 2 "','"0x10"','"0b11"','"0o7"','"1e3"','"Infinity"','"-Infinity"','"inf"','"abc"','"abd"','"\\uffff"','"\\ud83d\\ude00"','"2026-11-07T10:00:00.000Z"','"2026-11-08T10:00:00.000Z"','[]','[1]','[1,2]','[null]','{}','{"toString":1}','[{"toString":1}]','{"valueOf":1}','"+5"','"5."','".5"','"1_0"','"\\t3\\n"','"-0x10"','"+Infinity"'].filter(v=>v!=='1e400x');
const parse = (v) => v === 'undefined' ? undefined : JSON.parse(v);
const rows = [];
for (const a of vals) for (const b of vals) {
  let r; try { r = parse(a) < parse(b) ? 'T' : 'F'; } catch { r = 'E'; }
  rows.push(`${a}\t${b}\t${r}`);
}
console.log(rows.join('\n'));
