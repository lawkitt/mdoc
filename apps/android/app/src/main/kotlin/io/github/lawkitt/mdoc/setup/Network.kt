package io.github.lawkitt.mdoc.setup

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** How the phone reaches the internet, as far as model downloads care. */
enum class Route { WiFi, Cellular, None }

/**
 * The Wi-Fi and cellular networks, watched separately (ADR 0088, 0089):
 * "Wi-Fi" is a Wi-Fi or Ethernet transport, never the unmetered flag, which
 * Android 11+ also sets on some unlimited 5G plans. Downloads open their
 * connections on the network object itself, so a Wi-Fi download cannot
 * fall back to cellular.
 */
class NetworkWatch(context: Context) {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val wifiNetworks = linkedSetOf<Network>()
    private val cellularNetworks = linkedSetOf<Network>()
    private val mutable = MutableStateFlow(Route.None)
    val route: StateFlow<Route> = mutable.asStateFlow()

    val wifi: Network? @Synchronized get() = wifiNetworks.lastOrNull()
    val cellular: Network? @Synchronized get() = cellularNetworks.lastOrNull()

    init {
        watch(listOf(NetworkCapabilities.TRANSPORT_WIFI, NetworkCapabilities.TRANSPORT_ETHERNET), wifiNetworks)
        watch(listOf(NetworkCapabilities.TRANSPORT_CELLULAR), cellularNetworks)
    }

    private fun watch(transports: List<Int>, into: MutableSet<Network>) {
        for (transport in transports) {
            val request = NetworkRequest.Builder()
                .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                .addTransportType(transport)
                .build()
            connectivity.registerNetworkCallback(
                request,
                object : ConnectivityManager.NetworkCallback() {
                    override fun onAvailable(network: Network) = change { into += network }

                    override fun onLost(network: Network) = change { into -= network }
                },
            )
        }
    }

    @Synchronized
    private fun change(edit: () -> Unit) {
        edit()
        mutable.value = when {
            wifiNetworks.isNotEmpty() -> Route.WiFi
            cellularNetworks.isNotEmpty() -> Route.Cellular
            else -> Route.None
        }
    }
}
