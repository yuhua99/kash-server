mod lifecycle;
mod listing;
mod nicknames;

pub use lifecycle::{accept_friend, remove_friend, send_friend_request};
pub use listing::{ListFriendsQuery, SearchUsersQuery, list_friends, search_users};
pub use nicknames::update_nickname;

pub(crate) fn ordered_user_pair<'a>(user_a_id: &'a str, user_b_id: &'a str) -> (&'a str, &'a str) {
    if user_a_id < user_b_id {
        (user_a_id, user_b_id)
    } else {
        (user_b_id, user_a_id)
    }
}
