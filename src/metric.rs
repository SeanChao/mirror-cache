use metrics::*;

pub static COUNTER_CACHE_HIT: &str = "cache_hit";
pub static COUNTER_CACHE_MISS: &str = "cache_miss";
pub static COUNTER_TASKS_BG: &str = "download_tasks_bg";
pub static COUNTER_REQ: &str = "requests";
pub static COUNTER_REQ_SUCCESS: &str = "requests_success";
pub static COUNTER_REQ_FAILURE: &str = "requests_failure";
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
