package com.tmw.companion

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  companion object {
    @Volatile var readingForeground=false
    private var activeSince=0L
    private val foregroundRanges=mutableListOf<Pair<Long,Long>>()
    @Synchronized private fun foreground(value:Boolean) {
      if(value==readingForeground)return
      val now=System.currentTimeMillis()
      if(readingForeground && now>activeSince) {
        foregroundRanges.add(activeSince to now)
        if(foregroundRanges.size>128)foregroundRanges.removeAt(0)
      }
      readingForeground=value
      if(value)activeSince=now
    }
    @Synchronized fun activeIntervals(start:Long,end:Long):List<Pair<Long,Long>> {
      val ranges=foregroundRanges.toMutableList()
      if(readingForeground)ranges.add(activeSince to System.currentTimeMillis())
      return ranges.map {maxOf(start,it.first) to minOf(end,it.second)}.filter {it.second>it.first}
    }
  }
  private var resumed=false
  override fun onWindowFocusChanged(hasFocus:Boolean) {
    super.onWindowFocusChanged(hasFocus)
    foreground(resumed && hasFocus)
    lifecycle(if(readingForeground) "foreground" else "background")
  }
  private var readerWebView: WebView? = null
  override val handleBackNavigation: Boolean = true
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    readerWebView = webView
    ViewCompat.setOnApplyWindowInsetsListener(webView) { _, insets ->
      val safe = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout() or WindowInsetsCompat.Type.ime())
      val density = resources.displayMetrics.density
      val script = "document.documentElement.style.cssText += ';--native-safe-top:${safe.top / density}px;--native-safe-right:${safe.right / density}px;--native-safe-bottom:${safe.bottom / density}px;--native-safe-left:${safe.left / density}px';"
      webView.evaluateJavascript(script, null)
      insets
    }
    ViewCompat.requestApplyInsets(webView)
    webView.postDelayed({ ViewCompat.requestApplyInsets(webView) }, 500)
    webView.postDelayed({ ViewCompat.requestApplyInsets(webView) }, 1500)
  }
  private fun lifecycle(state: String) {
    readerWebView?.evaluateJavascript("window.dispatchEvent(new CustomEvent('tmw-lifecycle',{detail:'$state'}))", null)
  }
  override fun onPause() {
    resumed=false
    foreground(false)
    lifecycle("pause")
    super.onPause()
  }
  override fun onResume() {
    super.onResume()
    resumed=true
    foreground(hasWindowFocus())
    lifecycle(if(readingForeground) "resume" else "background")
    readerWebView?.let { ViewCompat.requestApplyInsets(it) }
  }
  override fun onDestroy() {
    readerWebView = null
    super.onDestroy()
  }
}
