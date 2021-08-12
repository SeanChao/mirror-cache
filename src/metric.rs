/// TODO: cache hit/miss for other endpoints
use metrics::*;

pub static COUNTER_PYPI_INDEX_REQUESTS: &str = "pypi_index_requests";
pub static COUNTER_PYPI_INDEX_REQ_SUCCESS: &str = "pypi_index_requests_success";
pub static COUNTER_PYPI_INDEX_REQ_FAILURE: &str = "pypi_index_requests_failure";
pub static COUNTER_PYPI_PKGS_REQ: &str = "pypi_pkgs_requests";
pub static COUNTER_ANACONDA_REQ: &str = "anaconda_requests";
pub static COUNTER_CACHE_HIT: &str = "cache_hit";
pub static COUNTER_CACHE_MISS: &str = "cache_miss";
pub static CNT_PYPI_INDEX_CACHE_HIT: &str = "cache_hit_pypi_index";
pub static CNT_PYPI_INDEX_CACHE_MISS: &str = "cache_miss_pypi_index";
pub static CNT_PYPI_PKGS_CACHE_HIT: &str = "cache_hit_pypi_pkgs";
pub static CNT_PYPI_PKGS_CACHE_MISS: &str = "cache_miss_pypi_pkgs";
pub static CNT_ANACONDA_CACHE_HIT: &str = "cache_hit_anaconda";
pub static CNT_ANACONDA_CACHE_MISS: &str = "cache_miss_anaconda";
pub static COUNTER_TASKS_BG: &str = "download_tasks_bg";
pub static CNT_TASKS_BG_SUCCESS: &str = "download_tasks_bg_success";
pub static CNT_TASKS_BG_FAILURE: &str = "download_tasks_bg_failure";
pub static CNT_OUT_REQUESTS: &str = "outbound_requests";
pub static CNT_OUT_REQUESTS_SUCCESS: &str = "outbound_requests_success";
pub static CNT_OUT_REQUESTS_FAILURE: &str = "outbound_requests_failure";
pub static HG_TASKS_LEN: &str = "current_download_tasks";
pub static HG_CACHE_SIZE_PREFIX: &str = "cache_size";
pub static CNT_RM_FILES: &str = "files_removed";

pub fn register_counters() {
    register_counter!(
        COUNTER_PYPI_INDEX_REQUESTS,
        "The number of requests to PyPI index endpoint."
    );
    register_counter!(
        COUNTER_PYPI_INDEX_REQ_SUCCESS,
        "The number of successful requests to PyPI index endpoint."
    );
    register_counter!(
        COUNTER_PYPI_INDEX_REQ_FAILURE,
        "The number of failed requests to PyPI index endpoint."
    );
    register_counter!(
        COUNTER_PYPI_PKGS_REQ,
        "The number of requests to PyPI packages endpoint."
    );
    register_counter!(COUNTER_ANACONDA_REQ, "The number of requests to Anaconda.");
    register_counter!(COUNTER_CACHE_HIT, "Overall cache hit count.");
    register_counter!(COUNTER_CACHE_MISS, "Overall cache miss count.");
    register_counter!(CNT_PYPI_INDEX_CACHE_HIT, "PyPI index cache hit count.");
    register_counter!(CNT_PYPI_INDEX_CACHE_MISS, "PyPI index cache miss count.");
    register_counter!(CNT_PYPI_PKGS_CACHE_HIT, "PyPI packages cache hit count.");
    register_counter!(CNT_PYPI_PKGS_CACHE_MISS, "PyPI packages cache miss count.");
    register_counter!(CNT_ANACONDA_CACHE_HIT, "Anaconda cache hit count.");
    register_counter!(CNT_ANACONDA_CACHE_MISS, "Anaconda cache miss count.");
    register_counter!(
        COUNTER_TASKS_BG,
        "The number of background download tasks spawned."
    );
    register_counter!(
        CNT_TASKS_BG_SUCCESS,
        "The number of successful background download tasks."
    );
    register_counter!(
        CNT_TASKS_BG_FAILURE,
        "The number of failed background download tasks."
    );
    register_counter!(CNT_OUT_REQUESTS, "The number of outbound requests.");
    register_counter!(
        CNT_OUT_REQUESTS_SUCCESS,
        "The number of successful outbound requests."
    );
    register_counter!(
        CNT_OUT_REQUESTS_FAILURE,
        "The number of failed outbound requests."
    );
    register_histogram!(
        HG_TASKS_LEN,
        "The current size of background download task set."
    );
    register_counter!(CNT_RM_FILES, "The number of removed files.");
}
