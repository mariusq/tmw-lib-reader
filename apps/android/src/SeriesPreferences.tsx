import { useEffect, useState } from "react";

import { readSeriesConfidence, setSeriesConfidence, type SeriesConfidence } from "./seriesPreferenceStorage";
export default function SeriesPreferences() {
  const [confidence, setConfidence] = useState(readSeriesConfidence);
  useEffect(() => {
    const update = () => setConfidence(readSeriesConfidence());
    window.addEventListener("tmw-series-preferences", update);
    return () => window.removeEventListener("tmw-series-preferences", update);
  }, []);
  return <section className="rounded-xl border border-slate-700 p-4"><h2 className="font-semibold">Series browsing</h2><label className="mt-3 block" htmlFor="phone-series-confidence">Series confidence</label><select id="phone-series-confidence" className="control mt-2 w-full bg-slate-800" value={confidence} onChange={event => setSeriesConfidence(event.target.value as SeriesConfidence)}><option value="high">High confidence only</option><option value="medium">High + medium confidence</option><option value="all">All candidates, including possible matches</option></select><p className="mt-3 text-sm text-slate-400">Applies to Consolidate by series in the library. Refresh from an updated PC once to cache series information. Browsing then works offline. Books, downloads and progress stay separate.</p></section>;
}
