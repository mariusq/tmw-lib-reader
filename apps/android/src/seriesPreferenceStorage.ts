export type SeriesConfidence = "high" | "medium" | "all";
export function readSeriesConfidence(): SeriesConfidence {
  const saved = localStorage.getItem("tmw-series-confidence");
  return saved === "medium" || saved === "all" ? saved : "high";
}
export function setSeriesConfidence(value: SeriesConfidence) {
  localStorage.setItem("tmw-series-confidence", value);
  window.dispatchEvent(new Event("tmw-series-preferences"));
}
