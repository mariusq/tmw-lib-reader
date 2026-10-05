package com.tmw.companion

import org.json.JSONObject

internal fun requireProtocol(response:JSONObject, expected:Int) {
    require(response.optInt("protocolVersion",-1)==expected) {
        "Incompatible PC sync protocol (phone requires v$expected). Update the PC and phone apps. Local books, saved data and queued changes are retained; you can export them in Settings."
    }
}

internal fun requireProtocolEndpoint(status:Int) {
    require(status!=404 && status!=426) {
        "Incompatible PC sync API. Update the PC and phone apps. Local books, saved data and queued changes are retained; export is available in Settings."
    }
}
