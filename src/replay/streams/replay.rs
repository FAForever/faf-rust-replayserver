use super::position::StreamPosition;

/* Common interface for a replay. Only declared for readability, we use concrete types. */
trait ReplayStream {
    fn wait(&self, until: StreamPosition) -> ();
}
