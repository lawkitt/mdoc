package io.github.lawkitt.mdoc

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.lawkitt.mdoc.home.HomeScreen
import io.github.lawkitt.mdoc.home.HomeViewModel
import io.github.lawkitt.mdoc.ui.MdocTheme

class MainActivity : ComponentActivity() {
    private val home: HomeViewModel by viewModels { HomeViewModel.factory(application as MdocApplication) }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContent {
            MdocTheme {
                val state by home.state.collectAsStateWithLifecycle()
                HomeScreen(
                    rows = state.rows,
                    canFindNames = state.canFindNames,
                    loaded = state.loaded,
                    error = state.error,
                    onRetry = home::refresh,
                )
            }
        }
    }

    override fun onStart() {
        super.onStart()
        home.refresh()
    }
}
