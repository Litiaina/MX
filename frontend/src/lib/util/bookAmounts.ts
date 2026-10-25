// Preview only. MX independently checks every amount and balance in its transaction.
export function decimalUnits(value:string,scale:number):bigint|null {
  if(!Number.isInteger(scale)||scale<0||scale>6||value.length>24||!/^\d+(?:\.\d+)?$/.test(value))return null;
  const [whole,fraction='']=value.split('.');if(fraction.length>scale)return null;
  const amount=BigInt(whole)*10n**BigInt(scale)+BigInt(fraction.padEnd(scale,'0')||'0');
  return amount>0n&&amount<=9223372036854775807n?amount:null;
}
export function formatUnits(value:bigint,scale:number):string {const negative=value<0n;const v=negative?-value:value;const digits=v.toString().padStart(scale+1,'0');return `${negative?'-':''}${scale?`${digits.slice(0,-scale)}.${digits.slice(-scale)}`:digits}`;}
export function journalTotals(lines:{debit:string;credit:string}[],scale:number){let debit=0n,credit=0n;for(const line of lines){const d=line.debit===''?0n:decimalUnits(line.debit,scale),c=line.credit===''?0n:decimalUnits(line.credit,scale);if(d===null||c===null||(d===0n)===(c===0n))return null;debit+=d;credit+=c;}return {debit:formatUnits(debit,scale),credit:formatUnits(credit,scale),balanced:debit===credit&&debit>0n};}
