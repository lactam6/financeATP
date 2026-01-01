//! User Aggregate
//!
//! User aggregate for managing user profile information.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{UserEvent, UserChanges};
use crate::error::AppError;

use super::Aggregate;

/// User status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserStatus {
    Active,
    Deactivated,
}

impl Default for UserStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// User Aggregate
/// 
/// Represents a user in the system.
/// Note: Authentication is handled by Next.js, this is only profile data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique user ID
    id: Uuid,
    
    /// Username (unique)
    username: String,
    
    /// Email (unique)
    email: String,
    
    /// Display name
    display_name: Option<String>,
    
    /// User status
    status: UserStatus,
    
    /// Current version
    version: i64,
    
    /// When the user was created
    created_at: Option<DateTime<Utc>>,
    
    /// When the user was last updated
    updated_at: Option<DateTime<Utc>>,
}

impl Default for User {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            username: String::new(),
            email: String::new(),
            display_name: None,
            status: UserStatus::Active,
            version: 0,
            created_at: None,
            updated_at: None,
        }
    }
}

impl User {
    // =========================================================================
    // M070: User::create()
    // =========================================================================
    
    /// Create a new user and generate the creation event
    pub fn create(
        user_id: Uuid,
        username: String,
        email: String,
        display_name: Option<String>,
    ) -> (Self, UserEvent) {
        let now = Utc::now();
        
        let event = UserEvent::UserCreated {
            user_id,
            username: username.clone(),
            email: email.clone(),
            display_name: display_name.clone(),
            created_at: now,
        };
        
        let user = Self {
            id: user_id,
            username,
            email,
            display_name,
            status: UserStatus::Active,
            version: 1,
            created_at: Some(now),
            updated_at: Some(now),
        };
        
        (user, event)
    }

    // =========================================================================
    // M072: User::update()
    // =========================================================================
    
    /// Update user profile
    pub fn update(&self, changes: UserChanges) -> Result<UserEvent, AppError> {
        if self.status == UserStatus::Deactivated {
            return Err(AppError::UserNotFound(self.id.to_string()));
        }
        
        // Check if there are any actual changes
        if changes.display_name.is_none() && changes.email.is_none() {
            return Err(AppError::InvalidRequest("No changes provided".to_string()));
        }
        
        Ok(UserEvent::UserUpdated {
            user_id: self.id,
            changes,
            updated_at: Utc::now(),
        })
    }

    /// Deactivate the user (soft delete)
    pub fn deactivate(&self, reason: Option<String>) -> Result<UserEvent, AppError> {
        if self.status == UserStatus::Deactivated {
            return Err(AppError::InvalidRequest("User is already deactivated".to_string()));
        }
        
        Ok(UserEvent::UserDeactivated {
            user_id: self.id,
            reason,
            deactivated_at: Utc::now(),
        })
    }

    /// Reactivate the user
    pub fn reactivate(&self) -> Result<UserEvent, AppError> {
        if self.status != UserStatus::Deactivated {
            return Err(AppError::InvalidRequest("User is not deactivated".to_string()));
        }
        
        Ok(UserEvent::UserReactivated {
            user_id: self.id,
            reactivated_at: Utc::now(),
        })
    }

    // =========================================================================
    // Getters
    // =========================================================================
    
    pub fn username(&self) -> &str {
        &self.username
    }
    
    pub fn email(&self) -> &str {
        &self.email
    }
    
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
    
    pub fn status(&self) -> &UserStatus {
        &self.status
    }
    
    pub fn is_active(&self) -> bool {
        self.status == UserStatus::Active
    }
    
    pub fn created_at(&self) -> Option<DateTime<Utc>> {
        self.created_at
    }
    
    pub fn updated_at(&self) -> Option<DateTime<Utc>> {
        self.updated_at
    }
}

// =========================================================================
// M071: User::apply()
// =========================================================================

impl Aggregate for User {
    type Event = UserEvent;

    fn aggregate_type() -> &'static str {
        "User"
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn version(&self) -> i64 {
        self.version
    }

    fn apply(mut self, event: Self::Event) -> Self {
        match event {
            UserEvent::UserCreated {
                user_id,
                username,
                email,
                display_name,
                created_at,
            } => {
                self.id = user_id;
                self.username = username;
                self.email = email;
                self.display_name = display_name;
                self.status = UserStatus::Active;
                self.created_at = Some(created_at);
                self.updated_at = Some(created_at);
            }
            
            UserEvent::UserUpdated { changes, updated_at, .. } => {
                if let Some(display_name) = changes.display_name {
                    self.display_name = Some(display_name);
                }
                if let Some(email) = changes.email {
                    self.email = email;
                }
                self.updated_at = Some(updated_at);
            }
            
            UserEvent::UserDeactivated { deactivated_at, .. } => {
                self.status = UserStatus::Deactivated;
                self.updated_at = Some(deactivated_at);
            }
            
            UserEvent::UserReactivated { reactivated_at, .. } => {
                self.status = UserStatus::Active;
                self.updated_at = Some(reactivated_at);
            }
        }
        
        self.version += 1;
        self
    }
}

// =========================================================================
// M073: User unit tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_create() {
        let user_id = Uuid::new_v4();
        
        let (user, event) = User::create(
            user_id,
            "alice".to_string(),
            "alice@example.com".to_string(),
            Some("Alice Smith".to_string()),
        );
        
        assert_eq!(user.id(), user_id);
        assert_eq!(user.username(), "alice");
        assert_eq!(user.email(), "alice@example.com");
        assert_eq!(user.display_name(), Some("Alice Smith"));
        assert_eq!(user.version(), 1);
        assert!(user.is_active());
        assert!(matches!(event, UserEvent::UserCreated { .. }));
    }

    #[test]
    fn test_user_update() {
        let user_id = Uuid::new_v4();
        let (user, _) = User::create(
            user_id,
            "alice".to_string(),
            "alice@example.com".to_string(),
            None,
        );
        
        let changes = UserChanges {
            display_name: Some("Alice Wonder".to_string()),
            email: None,
        };
        
        let event = user.update(changes).unwrap();
        assert!(matches!(event, UserEvent::UserUpdated { .. }));
        
        let user = user.apply(event);
        assert_eq!(user.display_name(), Some("Alice Wonder"));
        assert_eq!(user.version(), 2);
    }

    #[test]
    fn test_user_update_email() {
        let user_id = Uuid::new_v4();
        let (user, _) = User::create(
            user_id,
            "alice".to_string(),
            "alice@example.com".to_string(),
            None,
        );
        
        let changes = UserChanges {
            display_name: None,
            email: Some("alice.new@example.com".to_string()),
        };
        
        let event = user.update(changes).unwrap();
        let user = user.apply(event);
        
        assert_eq!(user.email(), "alice.new@example.com");
    }

    #[test]
    fn test_user_update_no_changes() {
        let user_id = Uuid::new_v4();
        let (user, _) = User::create(
            user_id,
            "alice".to_string(),
            "alice@example.com".to_string(),
            None,
        );
        
        let changes = UserChanges {
            display_name: None,
            email: None,
        };
        
        let result = user.update(changes);
        assert!(matches!(result, Err(AppError::InvalidRequest(_))));
    }

    #[test]
    fn test_user_deactivate() {
        let user_id = Uuid::new_v4();
        let (user, _) = User::create(
            user_id,
            "alice".to_string(),
            "alice@example.com".to_string(),
            None,
        );
        
        let event = user.deactivate(Some("User requested".to_string())).unwrap();
        let user = user.apply(event);
        
        assert!(!user.is_active());
        assert_eq!(user.status(), &UserStatus::Deactivated);
    }

    #[test]
    fn test_user_reactivate() {
        let user_id = Uuid::new_v4();
        let (user, _) = User::create(
            user_id,
            "alice".to_string(),
            "alice@example.com".to_string(),
            None,
        );
        
        // Deactivate
        let event = user.deactivate(None).unwrap();
        let user = user.apply(event);
        assert!(!user.is_active());
        
        // Reactivate
        let event = user.reactivate().unwrap();
        let user = user.apply(event);
        assert!(user.is_active());
    }

    #[test]
    fn test_deactivated_user_cannot_update() {
        let user_id = Uuid::new_v4();
        let (user, _) = User::create(
            user_id,
            "alice".to_string(),
            "alice@example.com".to_string(),
            None,
        );
        
        let event = user.deactivate(None).unwrap();
        let user = user.apply(event);
        
        let changes = UserChanges {
            display_name: Some("New Name".to_string()),
            email: None,
        };
        
        let result = user.update(changes);
        assert!(matches!(result, Err(AppError::UserNotFound(_))));
    }
}
