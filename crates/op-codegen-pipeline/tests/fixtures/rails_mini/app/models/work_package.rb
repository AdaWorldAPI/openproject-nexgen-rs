class WorkPackage < ApplicationRecord
  belongs_to :project
  has_many :time_entries

  validates :subject, presence: true
end
