/* A thin layer over rcheevos' rc_client so the Rust side only deals in a
 * handful of plain C calls. One client per process: slot only ever runs one
 * game at a time. Everything the client wants from the outside world (HTTP,
 * memory, events) is passed back to Rust through the hooks in slot_ch_hooks. */

#include <stdlib.h>
#include <string.h>

#include "rc_client.h"
#include "rc_consoles.h"
#include "rc_libretro.h"

typedef struct slot_ch_hooks {
    /* Perform an HTTP request on another thread, then call slot_ch_http_done(req, ...). */
    void (*http)(const char* url, const char* post_data, const char* content_type, void* req);
    /* An rc_client event, or the end of a login or game load. */
    void (*event)(int kind, uint32_t id, uint32_t points, const char* title, const char* detail);
    void (*log)(const char* message);
} slot_ch_hooks;

/* Event kinds handed to hooks.event. */
enum {
    SLOT_CH_LOGIN_OK = 1,
    SLOT_CH_LOGIN_FAILED = 2,
    SLOT_CH_GAME_LOADED = 3,
    SLOT_CH_GAME_FAILED = 4,
    SLOT_CH_UNLOCKED = 5,
    SLOT_CH_MASTERED = 6,
    SLOT_CH_SERVER_ERROR = 7,
    SLOT_CH_DISCONNECTED = 8,
    SLOT_CH_RECONNECTED = 9,
    SLOT_CH_RESET = 10,
    SLOT_CH_LEADERBOARD_SUBMITTED = 11,
};

typedef struct slot_ch_request {
    rc_client_server_callback_t callback;
    void* callback_data;
} slot_ch_request;

static rc_client_t* client;
static slot_ch_hooks hooks;
static rc_libretro_memory_regions_t regions;
static int regions_ready;
typedef void* (*slot_memory_data_fn)(unsigned id);
typedef size_t (*slot_memory_size_fn)(unsigned id);
static slot_memory_data_fn core_memory_data;
static slot_memory_size_fn core_memory_size;

static uint32_t RC_CCONV read_memory(uint32_t address, uint8_t* buffer, uint32_t num_bytes, rc_client_t* c)
{
    (void)c;
    if (!regions_ready)
        return 0;
    return rc_libretro_memory_read(&regions, address, buffer, num_bytes);
}

static void RC_CCONV server_call(const rc_api_request_t* request, rc_client_server_callback_t callback,
                                 void* callback_data, rc_client_t* c)
{
    slot_ch_request* req = malloc(sizeof(*req));
    (void)c;
    if (!req) {
        rc_api_server_response_t none = {0};
        none.http_status_code = RC_API_SERVER_RESPONSE_CLIENT_ERROR;
        callback(&none, callback_data);
        return;
    }
    req->callback = callback;
    req->callback_data = callback_data;
    hooks.http(request->url, request->post_data, request->content_type, req);
}

void slot_ch_http_done(void* handle, int status, const char* body, size_t body_length)
{
    slot_ch_request* req = handle;
    rc_api_server_response_t response;
    memset(&response, 0, sizeof(response));
    response.http_status_code = status;
    response.body = body;
    response.body_length = body_length;
    req->callback(&response, req->callback_data);
    free(req);
}

static void RC_CCONV log_message(const char* message, const rc_client_t* c)
{
    (void)c;
    if (hooks.log)
        hooks.log(message);
}

static void RC_CCONV handle_event(const rc_client_event_t* event, rc_client_t* c)
{
    (void)c;
    switch (event->type) {
    case RC_CLIENT_EVENT_ACHIEVEMENT_TRIGGERED:
        hooks.event(SLOT_CH_UNLOCKED, event->achievement->id, event->achievement->points,
                    event->achievement->title, event->achievement->description);
        break;
    case RC_CLIENT_EVENT_GAME_COMPLETED: {
        const rc_client_game_t* game = rc_client_get_game_info(client);
        hooks.event(SLOT_CH_MASTERED, game ? game->id : 0, 0, game ? game->title : "", "");
        break;
    }
    case RC_CLIENT_EVENT_LEADERBOARD_SUBMITTED:
        hooks.event(SLOT_CH_LEADERBOARD_SUBMITTED, event->leaderboard->id, 0, event->leaderboard->title,
                    event->leaderboard->tracker_value);
        break;
    case RC_CLIENT_EVENT_SERVER_ERROR:
        hooks.event(SLOT_CH_SERVER_ERROR, 0, 0, event->server_error->api, event->server_error->error_message);
        break;
    case RC_CLIENT_EVENT_DISCONNECTED:
        hooks.event(SLOT_CH_DISCONNECTED, 0, 0, "", "");
        break;
    case RC_CLIENT_EVENT_RECONNECTED:
        hooks.event(SLOT_CH_RECONNECTED, 0, 0, "", "");
        break;
    case RC_CLIENT_EVENT_RESET:
        hooks.event(SLOT_CH_RESET, 0, 0, "", "");
        break;
    default:
        break;
    }
}

int slot_ch_create(const slot_ch_hooks* h, const char* host)
{
    if (client)
        return 1;
    hooks = *h;
    client = rc_client_create(read_memory, server_call);
    if (!client)
        return 0;
    rc_client_enable_logging(client, RC_CLIENT_LOG_LEVEL_WARN, log_message);
    rc_client_set_event_handler(client, handle_event);
    rc_client_set_hardcore_enabled(client, 0);
    /* Memory is only valid on the emulator thread, inside slot_ch_do_frame. */
    rc_client_set_allow_background_memory_reads(client, 0);
    if (host && *host)
        rc_client_set_host(client, host);
    return 1;
}

void slot_ch_destroy(void)
{
    if (!client)
        return;
    rc_client_destroy(client);
    client = NULL;
    if (regions_ready)
        rc_libretro_memory_destroy(&regions);
    regions_ready = 0;
}

size_t slot_ch_user_agent(char* buffer, size_t size)
{
    return client ? rc_client_get_user_agent_clause(client, buffer, size) : 0;
}

void slot_ch_set_hardcore(int on)
{
    if (client)
        rc_client_set_hardcore_enabled(client, on);
}

int slot_ch_hardcore(void)
{
    return client ? rc_client_get_hardcore_enabled(client) : 0;
}

static void RC_CCONV login_done(int result, const char* error_message, rc_client_t* c, void* userdata)
{
    const rc_client_user_t* user;
    (void)userdata;
    if (result != RC_OK) {
        hooks.event(SLOT_CH_LOGIN_FAILED, (uint32_t)-result, 0, "", (error_message && *error_message) ? error_message : rc_error_str(result));
        return;
    }
    user = rc_client_get_user_info(c);
    hooks.event(SLOT_CH_LOGIN_OK, user->score, user->score_softcore, user->display_name, user->token);
}

void slot_ch_login_password(const char* user, const char* password)
{
    if (client)
        rc_client_begin_login_with_password(client, user, password, login_done, NULL);
}

void slot_ch_login_token(const char* user, const char* token)
{
    if (client)
        rc_client_begin_login_with_token(client, user, token, login_done, NULL);
}

static void RC_CCONV core_memory_info(uint32_t id, rc_libretro_core_memory_info_t* info)
{
    info->data = core_memory_data ? (uint8_t*)core_memory_data(id) : NULL;
    info->size = core_memory_size ? core_memory_size(id) : 0;
}

/* Point the client at the running core's memory. mmap may be NULL when the core
 * never announced its memory maps; rc_libretro then falls back to the core's
 * RETRO_MEMORY_SYSTEM_RAM and SAVE_RAM. Returns the number of regions found. */
uint32_t slot_ch_set_memory(const struct retro_memory_map* mmap, slot_memory_data_fn data,
                            slot_memory_size_fn size, uint32_t console_id)
{
    if (regions_ready)
        rc_libretro_memory_destroy(&regions);
    core_memory_data = data;
    core_memory_size = size;
    regions_ready = rc_libretro_memory_init(&regions, mmap, core_memory_info, console_id);
    return regions_ready ? regions.count : 0;
}

uint32_t slot_ch_memory_size(void)
{
    return regions_ready ? (uint32_t)regions.total_size : 0;
}

uint8_t* slot_ch_memory_at(uint32_t address)
{
    return regions_ready ? rc_libretro_memory_find(&regions, address) : NULL;
}

void slot_ch_forget_memory(void)
{
    if (regions_ready)
        rc_libretro_memory_destroy(&regions);
    regions_ready = 0;
    core_memory_data = NULL;
    core_memory_size = NULL;
}

static void RC_CCONV load_done(int result, const char* error_message, rc_client_t* c, void* userdata)
{
    const rc_client_game_t* game;
    rc_client_user_game_summary_t summary;
    (void)userdata;
    if (result != RC_OK) {
        hooks.event(SLOT_CH_GAME_FAILED, (uint32_t)-result, 0, "", (error_message && *error_message) ? error_message : rc_error_str(result));
        return;
    }
    game = rc_client_get_game_info(c);
    rc_client_get_user_game_summary(c, &summary);
    hooks.event(SLOT_CH_GAME_LOADED, summary.num_unlocked_achievements, summary.num_core_achievements,
                game ? game->title : "", "");
}

/* Identify the ROM from its bytes and fetch its achievements. */
void slot_ch_load_game(uint32_t console_id, const uint8_t* rom, size_t size)
{
    if (client)
        rc_client_begin_identify_and_load_game(client, console_id, NULL, rom, size, load_done, NULL);
}

void slot_ch_unload_game(void)
{
    if (client)
        rc_client_unload_game(client);
    slot_ch_forget_memory();
}

void slot_ch_do_frame(void)
{
    if (client && regions_ready)
        rc_client_do_frame(client);
}

void slot_ch_idle(void)
{
    if (client)
        rc_client_idle(client);
}

void slot_ch_reset(void)
{
    if (client)
        rc_client_reset(client);
}

size_t slot_ch_progress_size(void)
{
    return client ? rc_client_progress_size(client) : 0;
}

int slot_ch_save_progress(uint8_t* buffer, size_t size)
{
    return client ? rc_client_serialize_progress_sized(client, buffer, size) : RC_INVALID_STATE;
}

int slot_ch_load_progress(const uint8_t* buffer, size_t size)
{
    return client ? rc_client_deserialize_progress_sized(client, buffer, size) : RC_INVALID_STATE;
}
