export function backingOffer(metadata) {
  if(typeof metadata!=='string')return null;
  try {const value=JSON.parse(metadata);return value.offer??backingOffer(Array.isArray(value)?value[0]:value.project??value.original);}catch{return null;}
}
function takeTransferAdmission(buffers) {
  const admission=buffers?.admission;
  if(admission)delete buffers.admission;
  return admission;
}
export function releaseTransferred(buffers) {takeTransferAdmission(buffers)?.release();}
export function postTransferred(buffers,post) {
  const admission=takeTransferAdmission(buffers);
  try{return post();}finally{admission?.release();}
}
