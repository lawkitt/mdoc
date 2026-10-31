package io.github.lawkitt.mdoc.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import io.github.lawkitt.mdoc.MdocApplication
import io.github.lawkitt.mdoc.core.AppCore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.mdoc_ffi.ModelId

/** Home's view state, built from the core's snapshots. */
data class HomeState(
    val rows: List<HomeRow> = emptyList(),
    /** Above the detector floor: Find names is offered (ADR 0088, 0089). */
    val canFindNames: Boolean = true,
    val error: String? = null,
)

class HomeViewModel(private val core: () -> AppCore) : ViewModel() {
    private val mutable = MutableStateFlow(HomeState())
    val state: StateFlow<HomeState> = mutable.asStateFlow()

    /** Reads the library off the main thread. */
    fun refresh() {
        viewModelScope.launch(Dispatchers.IO) {
            mutable.value = runCatching {
                val core = core()
                HomeState(
                    rows = core.library.documents()
                        .sortedByDescending { it.modified }
                        .map { HomeRow(path = it.path, title = it.title) },
                    canFindNames = core.setup.catalog()
                        .any { it.id == ModelId.GLINER2_FP16 && it.offered },
                )
            }.getOrElse { HomeState(error = it.message ?: it.toString()) }
        }
    }

    companion object {
        fun factory(app: MdocApplication): ViewModelProvider.Factory = viewModelFactory {
            initializer { HomeViewModel { app.core } }
        }
    }
}
