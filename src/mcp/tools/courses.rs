//! Course-level tools: list_courses, get_course, get_announcements, get_grades.
//! Descriptions copied verbatim from server.mjs's tool() calls.

use super::{ensure_safe_id, fail, json_text};
use crate::mcp::EvusMcpServer;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CourseIdOnly {
    /// Blackboard course id
    pub course_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlainCourseId {
    pub course_id: String,
}

#[tool_router(router = tool_router_courses, vis = "pub(crate)")]
impl EvusMcpServer {
    #[rmcp::tool(description = "List the current user's enrolled courses", annotations(read_only_hint = true))]
    pub async fn list_courses(&self) -> Result<String, String> {
        let user_id = self.bb.user_id().await.map_err(fail)?;
        // This endpoint returns course *memberships* (courseId, role, ...),
        // not course details, so the name/title is missing unless the
        // nested course object is expanded in.
        self.bb
            .get(&format!(
                "/learn/api/public/v1/users/{user_id}/courses?expand=course"
            ))
            .await
            .map(json_text)
            .map_err(fail)
    }

    #[rmcp::tool(description = "Get details for a single course", annotations(read_only_hint = true))]
    pub async fn get_course(&self, Parameters(p): Parameters<CourseIdOnly>) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        self.bb
            .get(&format!("/learn/api/public/v3/courses/{}", p.course_id))
            .await
            .map(json_text)
            .map_err(fail)
    }

    #[rmcp::tool(description = "Get announcements for a course", annotations(read_only_hint = true))]
    pub async fn get_announcements(
        &self,
        Parameters(p): Parameters<PlainCourseId>,
    ) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        self.bb
            .get(&format!(
                "/learn/api/public/v1/courses/{}/announcements",
                p.course_id
            ))
            .await
            .map(json_text)
            .map_err(fail)
    }

    #[rmcp::tool(
        description = "Get the current user's grades in a course, from Blackboard's gradebook. \
            Some professors instead publish grades as a PDF/document in the course content \
            (e.g. named 'calificaciones', 'notas', 'acta', 'convocatoria') rather than \
            entering them in the gradebook. If this returns nothing for an exam/item \
            the user asks about, use find_attachment (query for those keywords) or \
            search_courses_content to look for a grades file instead of concluding \
            the grade doesn't exist.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_grades(&self, Parameters(p): Parameters<PlainCourseId>) -> Result<String, String> {
        ensure_safe_id(&p.course_id)?;
        let user_id = self.bb.user_id().await.map_err(fail)?;
        self.bb
            .get(&format!(
                "/learn/api/v1/courses/{}/gradebook/grades?userId={user_id}",
                p.course_id
            ))
            .await
            .map(json_text)
            .map_err(fail)
    }
}
